# Vendored engine sources

This directory holds the exact `crukx-core` workspace sources pinned by
`Cargo.lock` (`git+https://github.com/Crukx-dev/crukx-core?branch=main`),
so builds — including CI without private-repo credentials — never fetch
the engine from GitHub. Wired up by `.cargo/config.toml`.

Pinned engine rev: `4f8c757` (refresh together with `Cargo.lock`).

## Refresh after an engine update

Run from this repo root (needs read access to the private core repo):

```sh
cargo update -p crukx-core -p crukx-runner
REV=$(grep -A2 'name = "crukx-core"' Cargo.lock | grep -o '#[0-9a-f]\{40\}' | tr -d '#')
rm -rf /tmp/engine-vendor && cargo vendor /tmp/engine-vendor
rm -rf vendor/crukx-core vendor/crukx-runner vendor/crukx-storage
cp -r /tmp/engine-vendor/crukx-core /tmp/engine-vendor/crukx-runner \
  /tmp/engine-vendor/crukx-storage vendor/
sed -i "s/^Pinned engine rev: .*/Pinned engine rev: \`$REV\` (refresh together with \`Cargo.lock\`)./" vendor/README.md
cargo check --workspace
```

Then commit `Cargo.lock` + `vendor/` + this file together. Never hand-edit
files under `vendor/crukx-*` — cargo verifies `.cargo-checksum.json` on use.
