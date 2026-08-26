import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";

const stage = path.resolve(".tmp/package/dictation");
const manifest = JSON.parse(readFileSync("manifest.json", "utf8"));
rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });
for (const file of ["manifest.json", "icon.png"]) {
  execFileSync("powershell", ["-NoProfile", "-Command", `Copy-Item -LiteralPath '${file}' -Destination '${stage}'`]);
}
execFileSync("powershell", ["-NoProfile", "-Command", `Copy-Item -LiteralPath 'dist' -Destination '${stage}' -Recurse`]);
mkdirSync("release", { recursive: true });
const output = `release/dictation-${manifest.version}.kspkg`;
rmSync(output, { force: true });
const zip = `${output}.zip`;
rmSync(zip, { force: true });
execFileSync("tar", [
  "-a",
  "-c",
  "-f",
  zip,
  "-C",
  stage,
  "manifest.json",
  "icon.png",
  "dist",
]);
execFileSync("powershell", ["-NoProfile", "-Command", `Move-Item -LiteralPath '${zip}' -Destination '${output}'`]);
console.log(output);
