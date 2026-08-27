#!/usr/bin/env node
"use strict";

// crukx-rs is pre-1.0 with no release CI pipeline yet — there is nowhere
// to fetch a prebuilt binary from. The design spec's "Distribution"
// section (docs/superpowers/specs/2026-08-21-crukx-rs-design.md) plans a
// GitHub-Releases-based fetch here, per platform, checksum-verified. This
// shim is the honest version of that today: it finds and execs a locally
// built binary. When the release pipeline exists, this file gets a
// `fetchPrebuiltBinary()` step ahead of `findLocalBinary()` — nothing
// about the exec path below has to change.

const { spawnSync } = require("node:child_process");
const path = require("node:path");
const fs = require("node:fs");

function findLocalBinary() {
  // __dirname = crukx-rs/packages/crukx-js/bin -> crukx-rs/ is 3 levels up.
  const workspaceRoot = path.resolve(__dirname, "..", "..", "..");
  const candidates = [
    path.join(workspaceRoot, "target", "release", "crukx"),
    path.join(workspaceRoot, "target", "debug", "crukx"),
  ];
  return candidates.find((candidate) => fs.existsSync(candidate)) ?? null;
}

function main() {
  const binary = findLocalBinary();
  if (!binary) {
    console.error(
      [
        "crukx-rs: no built binary found.",
        "",
        "Run this first, from the crukx-rs/ directory:",
        "  cargo build --release",
        "",
        "Prebuilt binary distribution (no local Rust toolchain needed) is",
        "planned but not implemented yet — see",
        "docs/superpowers/specs/2026-08-21-crukx-rs-design.md → Distribution.",
      ].join("\n"),
    );
    process.exit(1);
  }

  const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) {
    console.error(`crukx-rs: failed to run ${binary}: ${result.error.message}`);
    process.exit(1);
  }
  process.exit(result.status ?? 1);
}

main();
