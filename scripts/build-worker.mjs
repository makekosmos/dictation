import { mkdirSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";

const buildDir = ".tmp/worker";
mkdirSync(buildDir, { recursive: true });
function runPnpm(args) {
  if (process.platform === "win32") {
    return spawnSync(
      process.env.ComSpec ?? "cmd.exe",
      ["/d", "/s", "/c", `pnpm.cmd ${args.join(" ")}`],
      { stdio: "inherit" },
    );
  }
  return spawnSync("pnpm", args, { stdio: "inherit" });
}

const compile = runPnpm([
  "exec",
  "tsc",
  "worker/worker.ts",
  "--outDir",
  buildDir,
  "--target",
  "ES2022",
  "--module",
  "NodeNext",
  "--moduleResolution",
  "NodeNext",
  "--ignoreConfig",
  "--types",
  "node",
  "--skipLibCheck",
]);
if (compile.status !== 0) process.exit(compile.status ?? 1);

const packageWorker = runPnpm([
  "exec",
  "pkg",
  `${buildDir}/worker.js`,
  "--targets",
  "host",
  "--output",
  "worker/dictation-worker.exe",
]);
rmSync(buildDir, { recursive: true, force: true });
if (packageWorker.status !== 0) process.exit(packageWorker.status ?? 1);
