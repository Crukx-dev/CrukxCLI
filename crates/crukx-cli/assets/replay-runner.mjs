// Runs under plain `node` (crukx-rs v1 scope: assertion files must be
// ESM JS — .mjs/.js — not TypeScript. The TS CLI supports .ts assertion
// files via `tsx`; a Rust-first CLI shouldn't require a Node TS-loader as
// a hard dependency just to check an assertion. See
// docs/superpowers/specs/2026-08-21-crukx-rs-design.md.
//
// argv: [assertionFile, contextFile]. Result is written as JSON to stdout.
import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

async function main() {
  const [assertionFile, contextFile] = process.argv.slice(2);
  if (!assertionFile || !contextFile) {
    throw new Error("Usage: replay-runner.mjs <assertion-file> <context-file>");
  }

  const context = JSON.parse(await readFile(contextFile, "utf8"));
  const moduleUrl = pathToFileURL(path.resolve(assertionFile)).href;
  const assertionModule = await import(moduleUrl);

  if (typeof assertionModule.assert !== "function") {
    throw new Error(`${assertionFile} does not export an "assert" function`);
  }

  const result = await assertionModule.assert(context);
  process.stdout.write(JSON.stringify(result));
}

main().catch((error) => {
  process.stdout.write(
    JSON.stringify({
      pass: false,
      reason: `assertion threw: ${error instanceof Error ? error.message : String(error)}`,
    }),
  );
  process.exitCode = 1;
});
