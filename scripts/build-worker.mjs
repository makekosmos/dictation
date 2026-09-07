import { mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";

mkdirSync("worker", { recursive: true });
const result = spawnSync(
  "bun",
  ["build", "--compile", "worker/worker.ts", "--outfile", "worker/dictation-worker.exe"],
  {
    stdio: "inherit",
    shell: true,
  },
);
if (result.status !== 0) process.exit(result.status ?? 1);
