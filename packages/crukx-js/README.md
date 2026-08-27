# @crukx/cli-rs

Preview npm shim for the Rust rewrite of the Crukx CLI (`crukx-rs/`).

## Status: local-build-only

There is no release CI pipeline yet, so there's nothing to download a
prebuilt binary from. `bin/crukx.js` finds and execs a binary you've
already built with `cargo build` — it does not fetch anything. This is a
temporary, honest state, not a design decision: the plan (see
[`docs/superpowers/specs/2026-08-21-crukx-rs-design.md`](../../../docs/superpowers/specs/2026-08-21-crukx-rs-design.md)
→ Distribution) is a GitHub-Releases-based fetch, checksum-verified, per
platform — `npm install -g @crukx/cli-rs` with zero local Rust toolchain
required. Nothing about `bin/crukx.js`'s exec path needs to change to get
there; only a fetch step gets prepended.

## Using it today

```bash
cd crukx-rs
cargo build --release   # or: cargo build (debug, faster to build, slower to run)
cd packages/crukx-js
npm link                # or: node bin/crukx.js gate --policy crukx.yml
crukx-rs --help
```

The npm-registered binary name is `crukx-rs`, not `crukx` — this package
installs alongside the TS CLI (`@crukx/cli`, binary `crukx`) without
colliding, since crukx-rs hasn't reached parity yet (see the design spec's
non-goals: capture/review/TUI are still TS-only).

## What actually works right now

`crukx-rs gate --policy crukx.yml` — the CI-critical path. Everything else
(`capture`, `replay`, `review`) is unimplemented in crukx-rs; use the TS
CLI (`crukx/packages/cli`) for those until crukx-rs reaches parity.
