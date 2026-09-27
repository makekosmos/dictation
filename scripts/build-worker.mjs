import { mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";

mkdirSync("worker", { recursive: true });
// package.manifest.json's worker target is windows/x86_64 only — compile for
// it explicitly, or a non-Windows host silently packs a host-native binary
// (e.g. a Linux ELF) as dictation-worker.exe.
const result = spawnSync(
  "bun",
  [
    "build",
    "--compile",
    "--target",
    "bun-windows-x64",
    "worker/worker.ts",
    "--outfile",
    "worker/dictation-worker.exe",
  ],
  {
    stdio: "inherit",
    shell: true,
  },
);
if (result.status !== 0) process.exit(result.status ?? 1);
