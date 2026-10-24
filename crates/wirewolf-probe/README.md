# wirewolf-probe

Part of [wirewolf](https://github.com/joshka/wirewolf), a self-hosted network diagnostics
service that tells you whether your network is broken, where, and since when.

The live probes (gateway, ISP, internet, DNS, web, user targets), the speed tests, and SQLite history.

**Most people want the [`wirewolf`](https://crates.io/crates/wirewolf) crate**, which installs
the binary: `cargo install wirewolf`. This crate is an internal part of it, published so that
`wirewolf` can depend on it. Its API is not stable and may change in any release.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
