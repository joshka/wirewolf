# Contributing

Thanks for helping with wirewolf. `AGENTS.md` has the short version of how the repo works:
building, testing, docs, images, and commit messages. This file covers the parts that need
more explanation.

## Changelog

`CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Entries are
written as changes land, not at release time:

- Any change a user would notice adds a line under `[Unreleased]`, in the same commit, in the
  right group (Added, Changed, Fixed, Removed, Security). Internal refactors and test-only
  changes don't need one.
- Write for someone upgrading: what they'll see or can now do, not what the code does. "Speed
  results show as a share of your plan", not "Add PlanShare component".
- Name anything they have to set or call: config keys (`speed.plan_down_mbps`), endpoints
  (`/api/v1/metrics`), CLI flags, and the API version when the contract changes.
- Say how to turn a feature on if it's off by default, and call out anything that changes
  existing behavior or needs action on upgrade.
- Fold follow-up fixes to something not yet released into its existing entry instead of adding
  a new line.

To release, rename `[Unreleased]` to the version and date, add a fresh empty `[Unreleased]`
above it, bump the versions, and use that section as the GitHub release notes.
