//! ICMP echo and TTL-limited probes over IPv4, without root where the OS allows it.
//!
//! macOS allows unprivileged `SOCK_DGRAM` ICMP sockets and delivers both echo replies and
//! time-exceeded messages to them. Linux allows them when `net.ipv4.ping_group_range` covers the
//! user, delivers echo replies normally, and queues time-exceeded messages on the socket error
//! queue when `IP_RECVERR` is set. [`Icmp::detect`] picks the best socket type available, and on
//! Linux sends one TTL-1 probe to check that time-exceeded replies really arrive.
//!
//! All calls block; run them on a blocking thread.

use std::io;
use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icmp {
    /// A raw socket: echo and time-exceeded both work.
    Raw,
    /// An unprivileged datagram socket. `ttl_exceeded` says whether time-exceeded replies arrive.
    Dgram { ttl_exceeded: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reply {
    pub from: IpAddr,
    pub rtt: Duration,
    /// The reply came from the target itself (echo reply), not a router on the way.
    pub reached: bool,
}

impl Icmp {
    /// Returns the best ICMP socket type this process may open, or `None` if ICMP is unavailable.
    ///
    /// On Linux a datagram socket only counts as able to trace if a TTL-1 probe toward `probe`
    /// brings back a time-exceeded reply, which takes up to `timeout`.
    pub fn detect(probe: Ipv4Addr, timeout: Duration) -> Option<Self> {
        if Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)).is_ok() {
            return Some(Self::Raw);
        }
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).ok()?;
        if cfg!(target_vendor = "apple") {
            return Some(Self::Dgram { ttl_exceeded: true });
        }
        if !cfg!(target_os = "linux") {
            return Some(Self::Dgram { ttl_exceeded: false });
        }
        let traced = Self::Dgram { ttl_exceeded: true }.burst(probe, 1, 1, Duration::ZERO, timeout);
        let ttl_exceeded = matches!(traced.as_deref(), Ok([Some(_)]));
        Some(Self::Dgram { ttl_exceeded })
    }

    /// Whether TTL-limited probes can see which router answered.
    pub fn can_trace(self) -> bool {
        matches!(self, Self::Raw | Self::Dgram { ttl_exceeded: true })
    }

    fn socket(self) -> io::Result<Socket> {
        let ty = match self {
            Self::Raw => Type::RAW,
            Self::Dgram { .. } => Type::DGRAM,
        };
        let socket = Socket::new(Domain::IPV4, ty, Some(Protocol::ICMPV4))?;
        #[cfg(target_os = "linux")]
        if matches!(self, Self::Dgram { ttl_exceeded: true }) {
            set_recverr(&socket)?;
        }
        Ok(socket)
    }

    /// Sends `count` echo requests `spacing` apart with the given TTL, and returns one entry per
    /// request: the reply, or `None` if nothing came back within `timeout` of sending.
    pub fn burst(self, target: Ipv4Addr, ttl: u32, count: u16, spacing: Duration, timeout: Duration) -> io::Result<Vec<Option<Reply>>> {
        let socket = self.socket()?;
        socket.set_ttl_v4(ttl)?;
        let id = fastrand::u16(..);
        let dest = SocketAddr::from((target, 0)).into();
        let mut sent: Vec<Instant> = Vec::with_capacity(count as usize);
        let mut replies = vec![None; count as usize];
        let start = Instant::now();
        let mut buf = [MaybeUninit::<u8>::uninit(); 1500];
        loop {
            let now = Instant::now();
            // Send the next request when it is due.
            if sent.len() < count as usize && now >= start + spacing * sent.len() as u32 {
                let seq = sent.len() as u16;
                socket.send_to(&echo_request(id, seq), &dest)?;
                sent.push(Instant::now());
                continue;
            }
            let deadline = if sent.len() < count as usize {
                start + spacing * sent.len() as u32
            } else {
                *sent.last().unwrap() + timeout
            };
            if now >= deadline {
                if sent.len() == count as usize {
                    break;
                }
                continue;
            }
            set_read_timeout(&socket, deadline - now)?;
            let (kind, reply_id, seq, from) = match socket.recv_from(&mut buf) {
                Ok((len, from)) => {
                    // SAFETY: recv_from initialized the first `len` bytes.
                    let data = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), len) };
                    let Some((kind, reply_id, seq)) = parse(data) else { continue };
                    (kind, reply_id, seq, from.as_socket().map(|s| s.ip()))
                }
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
                // A queued ICMP error makes recv fail with its errno (EHOSTUNREACH for
                // time-exceeded); the message itself waits on the error queue.
                #[cfg(target_os = "linux")]
                Err(_) if matches!(self, Self::Dgram { ttl_exceeded: true }) => {
                    let Some((seq, from)) = recv_time_exceeded(&socket) else { continue };
                    (ReplyKind::TimeExceeded, id, seq, Some(IpAddr::V4(from)))
                }
                Err(e) => return Err(e),
            };
            let received = Instant::now();
            let check_id = self == Self::Raw || cfg!(target_vendor = "apple");
            if (check_id && reply_id != id) || seq as usize >= sent.len() {
                continue;
            }
            let rtt = received - sent[seq as usize];
            if rtt > timeout || replies[seq as usize].is_some() {
                continue;
            }
            let from = from.unwrap_or(IpAddr::V4(target));
            replies[seq as usize] = Some(Reply { from, rtt, reached: kind == ReplyKind::Echo });
        }
        Ok(replies)
    }

    /// Sends one don't-fragment echo request of `size` bytes (IP header included) and reports
    /// whether the reply came back. Too big for the local link or the path is `false`.
    pub fn fits(self, target: Ipv4Addr, size: usize, timeout: Duration) -> io::Result<bool> {
        let socket = self.socket()?;
        set_dont_fragment(&socket)?;
        let id = fastrand::u16(..);
        let dest = SocketAddr::from((target, 0)).into();
        match socket.send_to(&sized_echo_request(id, 0, size.saturating_sub(28)), &dest) {
            Ok(_) => {}
            // EMSGSIZE: bigger than the interface (or a cached path) MTU.
            Err(e) if e.raw_os_error() == Some(libc::EMSGSIZE) => return Ok(false),
            Err(e) => return Err(e),
        }
        let deadline = Instant::now() + timeout;
        let mut buf = [MaybeUninit::<u8>::uninit(); 9100];
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            set_read_timeout(&socket, deadline - now)?;
            let len = match socket.recv_from(&mut buf) {
                Ok((len, _)) => len,
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => return Ok(false),
                Err(e) if e.raw_os_error() == Some(libc::EMSGSIZE) => return Ok(false),
                Err(e) => return Err(e),
            };
            // SAFETY: recv_from initialized the first `len` bytes.
            let data = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), len) };
            let check_id = self == Self::Raw || cfg!(target_vendor = "apple");
            if let Some((ReplyKind::Echo, reply_id, 0)) = parse(data)
                && (!check_id || reply_id == id)
            {
                return Ok(true);
            }
        }
    }

    /// An mtr-style sweep toward `target`: one probe per TTL from 1 to `max_ttl`, stopping once the
    /// target answers. Each entry is `(ttl, reply)`.
    pub fn trace(self, target: Ipv4Addr, max_ttl: u32, timeout: Duration) -> io::Result<Vec<(u32, Option<Reply>)>> {
        let mut hops = Vec::new();
        for ttl in 1..=max_ttl {
            let reply = self.burst(target, ttl, 1, Duration::ZERO, timeout)?.pop().flatten();
            let reached = reply.is_some_and(|r| r.reached);
            hops.push((ttl, reply));
            if reached {
                break;
            }
        }
        Ok(hops)
    }
}

/// Sets how long the next receive may block. A timeout that rounds to zero would mean "block
/// forever" (`SO_RCVTIMEO` of 0), which hung probes whenever a deadline was under a microsecond
/// away and nothing arrived, so it is never shorter than a millisecond.
fn set_read_timeout(socket: &Socket, remaining: Duration) -> io::Result<()> {
    socket.set_read_timeout(Some(remaining.max(Duration::from_millis(1))))
}

/// Sets don't-fragment, ignoring any cached path MTU so each probe tests the path afresh.
fn set_dont_fragment(socket: &Socket) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    #[cfg(target_os = "linux")]
    let (opt, value) = (libc::IP_MTU_DISCOVER, libc::IP_PMTUDISC_PROBE);
    #[cfg(not(target_os = "linux"))]
    let (opt, value) = (libc::IP_DONTFRAG, 1);
    // SAFETY: a valid socket, and a c_int option value of the right size.
    let r = unsafe {
        libc::setsockopt(socket.as_raw_fd(), libc::IPPROTO_IP, opt, (&raw const value).cast(), size_of::<libc::c_int>() as libc::socklen_t)
    };
    if r == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// Asks Linux to queue ICMP errors (time-exceeded included) for reading with `MSG_ERRQUEUE`.
#[cfg(target_os = "linux")]
fn set_recverr(socket: &Socket) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let on: libc::c_int = 1;
    // SAFETY: a valid socket, and a c_int option value of the right size.
    let r = unsafe {
        libc::setsockopt(socket.as_raw_fd(), libc::IPPROTO_IP, libc::IP_RECVERR, (&raw const on).cast(), size_of::<libc::c_int>() as libc::socklen_t)
    };
    if r == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// Reads one message from the error queue and returns (seq, router) if it is a time-exceeded
/// reply to one of our echo requests.
#[cfg(target_os = "linux")]
fn recv_time_exceeded(socket: &Socket) -> Option<(u16, Ipv4Addr)> {
    use std::os::fd::AsRawFd;
    let mut data = [0u8; 576];
    let mut control = [0u64; 64];
    let mut iov = libc::iovec { iov_base: data.as_mut_ptr().cast(), iov_len: data.len() };
    // SAFETY: an all-zero msghdr is valid; the pointers set below outlive the call.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &raw mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = size_of_val(&control) as _;
    // SAFETY: a valid socket and a msghdr whose buffers are live and correctly sized.
    let len = unsafe { libc::recvmsg(socket.as_raw_fd(), &raw mut msg, libc::MSG_ERRQUEUE | libc::MSG_DONTWAIT) };
    let len = usize::try_from(len).ok()?;
    // SAFETY: `control` is a plain byte buffer of at least `msg_controllen` bytes.
    let control = unsafe { std::slice::from_raw_parts(control.as_ptr().cast::<u8>(), msg.msg_controllen as usize) };
    let from = time_exceeded_offender(control)?;
    // The payload is the echo request we sent (ICMP header first, no IP header).
    let sent = data.get(..len)?;
    (*sent.first()? == 8).then_some(())?;
    Some((u16::from_be_bytes([*sent.get(6)?, *sent.get(7)?]), from))
}

/// Finds the `IP_RECVERR` control message in `control` and returns the router that sent a
/// time-exceeded reply, from `SO_EE_OFFENDER`. The layout is Linux's `cmsghdr`, then
/// `sock_extended_err`, then the offender's `sockaddr_in`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn time_exceeded_offender(control: &[u8]) -> Option<Ipv4Addr> {
    const SOL_IP: i32 = 0;
    const IP_RECVERR: i32 = 11;
    const SO_EE_ORIGIN_ICMP: u8 = 2;
    const AF_INET: u16 = 2;
    const WORD: usize = size_of::<usize>();
    let align = |n: usize| n.div_ceil(WORD) * WORD;
    let header = align(WORD + 8);
    let mut rest = control;
    while rest.len() >= header {
        let len = usize::from_ne_bytes(rest[..WORD].try_into().ok()?);
        let level = i32::from_ne_bytes(rest[WORD..WORD + 4].try_into().ok()?);
        let kind = i32::from_ne_bytes(rest[WORD + 4..WORD + 8].try_into().ok()?);
        if len < header || len > rest.len() {
            return None;
        }
        if level == SOL_IP && kind == IP_RECVERR {
            // sock_extended_err: errno u32, origin, type, code, pad, info u32, data u32.
            let ee = rest.get(header..len)?;
            let offender = ee.get(16..24)?;
            let family = u16::from_ne_bytes([offender[0], offender[1]]);
            return (ee.get(4) == Some(&SO_EE_ORIGIN_ICMP) && ee.get(5) == Some(&11) && family == AF_INET)
                .then(|| Ipv4Addr::new(offender[4], offender[5], offender[6], offender[7]));
        }
        rest = rest.get(align(len)..)?;
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplyKind {
    Echo,
    TimeExceeded,
}

fn echo_request(id: u16, seq: u16) -> Vec<u8> {
    sized_echo_request(id, seq, 16)
}

/// An echo request with `payload` bytes after the 8-byte header.
fn sized_echo_request(id: u16, seq: u16, payload: usize) -> Vec<u8> {
    let mut packet = vec![8, 0, 0, 0];
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&seq.to_be_bytes());
    packet.extend(b"wirewolf-probe..".iter().cycle().take(payload));
    let sum = checksum(&packet);
    packet[2..4].copy_from_slice(&sum.to_be_bytes());
    packet
}

fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = data.chunks(2).map(|c| u32::from(u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))).sum();
    while sum > 0xffff {
        sum = (sum >> 16) + (sum & 0xffff);
    }
    !(sum as u16)
}

/// Skips an IPv4 header if the packet starts with one. Raw sockets and macOS datagram sockets
/// include it; Linux datagram sockets don't.
fn strip_ip(data: &[u8]) -> Option<&[u8]> {
    match data.first()? >> 4 {
        4 => data.get(usize::from(data[0] & 0x0f) * 4..),
        _ => Some(data),
    }
}

/// Parses an ICMP message into (kind, id, seq) of the echo request it answers.
fn parse(data: &[u8]) -> Option<(ReplyKind, u16, u16)> {
    let icmp = strip_ip(data)?;
    let id_seq = |m: &[u8]| Some((u16::from_be_bytes([*m.get(4)?, *m.get(5)?]), u16::from_be_bytes([*m.get(6)?, *m.get(7)?])));
    match *icmp.first()? {
        0 => id_seq(icmp).map(|(id, seq)| (ReplyKind::Echo, id, seq)),
        11 => {
            // Time exceeded: 8 bytes of header, then the original IP header and ICMP header.
            let inner = icmp.get(8..)?;
            let inner_icmp = inner.get(usize::from(inner.first()? & 0x0f) * 4..)?;
            (*inner_icmp.first()? == 8).then_some(())?;
            id_seq(inner_icmp).map(|(id, seq)| (ReplyKind::TimeExceeded, id, seq))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_of_request_verifies() {
        let packet = echo_request(0x1234, 7);
        assert_eq!(checksum(&packet), 0);
    }

    #[test]
    fn parses_echo_reply_with_and_without_ip_header() {
        let mut reply = echo_request(0xbeef, 3);
        reply[0] = 0;
        assert_eq!(parse(&reply), Some((ReplyKind::Echo, 0xbeef, 3)));
        let mut with_ip = vec![0x45; 1];
        with_ip.extend([0u8; 19]);
        with_ip.extend(&reply);
        assert_eq!(parse(&with_ip), Some((ReplyKind::Echo, 0xbeef, 3)));
    }

    #[test]
    fn parses_time_exceeded() {
        let mut packet = vec![0x45];
        packet.extend([0u8; 19]);
        packet.extend([11, 0, 0, 0, 0, 0, 0, 0]);
        packet.push(0x45);
        packet.extend([0u8; 19]);
        packet.extend(&echo_request(42, 9)[..8]);
        assert_eq!(parse(&packet), Some((ReplyKind::TimeExceeded, 42, 9)));
    }

    /// A Linux `IP_RECVERR` control message for a time-exceeded reply from `from`.
    fn recverr_cmsg(origin: u8, icmp_type: u8, from: [u8; 4]) -> Vec<u8> {
        let word = size_of::<usize>();
        let header = (word + 8).div_ceil(word) * word;
        let mut ee = vec![0u8; 16];
        ee[..4].copy_from_slice(&113u32.to_ne_bytes()); // EHOSTUNREACH
        ee[4] = origin;
        ee[5] = icmp_type;
        ee.extend(2u16.to_ne_bytes());
        ee.extend([0, 0]);
        ee.extend(from);
        ee.extend([0u8; 8]);
        let mut cmsg = (header + ee.len()).to_ne_bytes().to_vec();
        cmsg.extend(0i32.to_ne_bytes());
        cmsg.extend(11i32.to_ne_bytes());
        cmsg.resize(header, 0);
        cmsg.extend(ee);
        cmsg
    }

    #[test]
    fn reads_time_exceeded_offender() {
        let cmsg = recverr_cmsg(2, 11, [192, 0, 2, 1]);
        assert_eq!(time_exceeded_offender(&cmsg), Some(Ipv4Addr::new(192, 0, 2, 1)));
    }

    #[test]
    fn skips_other_control_messages() {
        let word = size_of::<usize>();
        let mut control = (word + 8 + 4).to_ne_bytes().to_vec();
        control.extend(0i32.to_ne_bytes());
        control.extend(2i32.to_ne_bytes()); // IP_TTL
        control.extend(64i32.to_ne_bytes());
        control.resize((word + 12).div_ceil(word) * word, 0);
        control.extend(recverr_cmsg(2, 11, [198, 51, 100, 7]));
        assert_eq!(time_exceeded_offender(&control), Some(Ipv4Addr::new(198, 51, 100, 7)));
    }

    #[test]
    fn ignores_other_errors() {
        assert_eq!(time_exceeded_offender(&recverr_cmsg(2, 3, [192, 0, 2, 1])), None); // unreachable
        assert_eq!(time_exceeded_offender(&recverr_cmsg(1, 11, [192, 0, 2, 1])), None); // local
        assert_eq!(time_exceeded_offender(&recverr_cmsg(2, 11, [192, 0, 2, 1])[..20]), None);
        assert_eq!(time_exceeded_offender(&[]), None);
    }

    /// Needs a network. `cargo test -p wirewolf-probe -- --ignored live_trace --nocapture`
    #[test]
    #[ignore]
    fn live_trace() {
        let icmp = Icmp::detect(Ipv4Addr::new(1, 1, 1, 1), Duration::from_secs(1)).expect("ICMP available");
        let hops = icmp.trace(Ipv4Addr::new(1, 1, 1, 1), 12, Duration::from_secs(1)).unwrap();
        for (ttl, reply) in &hops {
            println!("{ttl:2} {reply:?}");
        }
        assert!(hops.last().unwrap().1.is_some_and(|r| r.reached));
    }
}

#[cfg(test)]
mod read_timeout_tests {
    use super::*;

    #[test]
    fn a_tiny_remaining_time_still_times_out() {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None).unwrap();
        socket.bind(&SocketAddr::from(([127, 0, 0, 1], 0)).into()).unwrap();
        set_read_timeout(&socket, Duration::from_nanos(1)).unwrap();
        let mut buf = [MaybeUninit::<u8>::uninit(); 16];
        let start = Instant::now();
        assert!(socket.recv_from(&mut buf).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
