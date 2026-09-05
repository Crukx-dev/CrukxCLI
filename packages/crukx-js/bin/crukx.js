#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const path = require("node:path");
const fs = require("node:fs");
const os = require("node:os");
const https = require("node:https");

const REPO = "crukx/crukx-rs";
const BINARY_NAME = "crukx";

function getPlatformInfo() {
  const platform = os.platform();
  const arch = os.arch();

  const platformMap = {
    linux: "unknown-linux-gnu",
    darwin: "apple-darwin",
    win32: "pc-windows-msvc",
  };

  const archMap = {
    x64: "x86_64",
    arm64: "aarch64",
  };

  const targetPlatform = platformMap[platform];
  const targetArch = archMap[arch];

  if (!targetPlatform || !targetArch) {
    return null;
  }

  // Special case: Linux musl for Alpine/containers
  if (platform === "linux") {
    try {
      const ldd = spawnSync("ldd", ["--version"], { encoding: "utf8" });
      if (ldd.stdout && ldd.stdout.includes("musl")) {
        return `${targetArch}-unknown-linux-musl`;
      }
    } catch {}
  }

  return `${targetArch}-${targetPlatform}`;
}

function getBinaryExtension() {
  return os.platform() === "win32" ? ".exe" : "";
}

function getArchiveExtension() {
  return os.platform() === "win32" ? ".zip" : ".tar.gz";
}

function getCacheDir() {
  const home = os.homedir();
  const platform = os.platform();

  if (platform === "win32") {
    return path.join(process.env.LOCALAPPDATA || path.join(home, "AppData", "Local"), "crukx");
  }

  if (platform === "darwin") {
    return path.join(home, "Library", "Caches", "crukx");
  }

  return path.join(process.env.XDG_CACHE_HOME || path.join(home, ".cache"), "crukx");
}

function getCachedBinaryPath() {
  const cacheDir = getCacheDir();
  const ext = getBinaryExtension();
  return path.join(cacheDir, `bin`, `${BINARY_NAME}${ext}`);
}

function findLocalBinary() {
  const workspaceRoot = path.resolve(__dirname, "..", "..", "..");
  const candidates = [
    path.join(workspaceRoot, "target", "release", `${BINARY_NAME}${getBinaryExtension()}`),
    path.join(workspaceRoot, "target", "debug", `${BINARY_NAME}${getBinaryExtension()}`),
  ];
  return candidates.find((candidate) => fs.existsSync(candidate)) ?? null;
}

function findCachedBinary() {
  const cached = getCachedBinaryPath();
  return fs.existsSync(cached) ? cached : null;
}

function downloadFile(url) {
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { "User-Agent": "crukx-js" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        downloadFile(res.headers.location).then(resolve).catch(reject);
        return;
      }

      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode}: ${url}`));
        return;
      }

      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => resolve(Buffer.concat(chunks)));
      res.on("error", reject);
    }).on("error", reject);
  });
}

async function getLatestVersion() {
  const url = `https://api.github.com/repos/${REPO}/releases/latest`;
  const data = await downloadFile(url);
  const release = JSON.parse(data.toString());
  return release.tag_name;
}

async function fetchPrebuiltBinary() {
  const target = getPlatformInfo();
  if (!target) {
    console.error("crukx-rs: unsupported platform/architecture combination");
    return null;
  }

  const cacheDir = getCacheDir();
  const binDir = path.join(cacheDir, "bin");
  const binaryPath = getCachedBinaryPath();

  try {
    console.error(`crukx-rs: fetching prebuilt binary for ${target}...`);

    const version = await getLatestVersion();
    const ext = getArchiveExtension();
    const archiveName = `${BINARY_NAME}-${target}${ext}`;
    const url = `https://github.com/${REPO}/releases/download/${version}/${archiveName}`;

    console.error(`crukx-rs: downloading ${url}`);
    const archive = await downloadFile(url);

    fs.mkdirSync(binDir, { recursive: true });

    const archivePath = path.join(cacheDir, archiveName);
    fs.writeFileSync(archivePath, archive);

    if (ext === ".tar.gz") {
      spawnSync("tar", ["xzf", archivePath, "-C", binDir], { stdio: "inherit" });
    } else {
      spawnSync("unzip", ["-o", archivePath, "-d", binDir], { stdio: "inherit" });
    }

    fs.unlinkSync(archivePath);

    if (os.platform() !== "win32") {
      fs.chmodSync(binaryPath, 0o755);
    }

    console.error(`crukx-rs: installed to ${binaryPath}`);
    return binaryPath;
  } catch (err) {
    console.error(`crukx-rs: failed to fetch prebuilt binary: ${err.message}`);
    return null;
  }
}

async function main() {
  let binary = findLocalBinary();

  if (!binary) {
    binary = findCachedBinary();
  }

  if (!binary) {
    binary = await fetchPrebuiltBinary();
  }

  if (!binary) {
    console.error(
      [
        "crukx-rs: no binary available.",
        "",
        "Options:",
        "  1. Build from source (requires Rust):",
        "     cargo build --release",
        "",
        "  2. Install from crates.io:",
        "     cargo install crukx --version 0.1.0-alpha.0 --locked",
        "",
        "  3. Download prebuilt binary manually:",
        `     https://github.com/${REPO}/releases`,
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

main().catch((err) => {
  console.error(`crukx-rs: unexpected error: ${err.message}`);
  process.exit(1);
});
