import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync } from "node:fs";
import path from "node:path";

const stage = path.resolve(".tmp/kext/dictation");
rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });
for (const file of ["manifest.json", "icon.svg"]) {
  execFileSync("powershell", ["-NoProfile", "-Command", `Copy-Item -LiteralPath '${file}' -Destination '${stage}'`]);
}
execFileSync("powershell", ["-NoProfile", "-Command", `Copy-Item -LiteralPath 'dist' -Destination '${stage}' -Recurse`]);
mkdirSync("release", { recursive: true });
execFileSync("tar", ["-a", "-c", "-f", "release/dictation-0.1.0.kext", "-C", stage, "."]);
console.log("release/dictation-0.1.0.kext");
