import { execFileSync } from "node:child_process";
import { cp, mkdir, readFile, rename, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(await readFile(path.join(root, "manifest.json"), "utf8"));
const stage = path.join(root, ".tmp", "package", "dictation");
const releaseDir = path.join(root, "release");
const output = path.join(releaseDir, `dictation-${manifest.version}.kspkg`);
const archiveTmp = `${output}.zip`;

await rm(stage, { recursive: true, force: true });
await rm(output, { force: true });
await rm(archiveTmp, { force: true });
await mkdir(stage, { recursive: true });
await mkdir(releaseDir, { recursive: true });
await cp(path.join(root, "manifest.json"), path.join(stage, "manifest.json"));
await cp(path.join(root, "icon.png"), path.join(stage, "icon.png"));
await cp(path.join(root, "dist"), path.join(stage, "dist"), { recursive: true });

if (process.platform === "win32") {
  const archiveArg = path.relative(root, archiveTmp).replaceAll(path.sep, "/");
  const stageArg = path.relative(root, stage).replaceAll(path.sep, "/");
  execFileSync(
    "tar",
    ["-a", "-c", "-f", archiveArg, "-C", stageArg, "manifest.json", "icon.png", "dist"],
    { cwd: root, stdio: "inherit" },
  );
} else {
  execFileSync("zip", ["-qr", archiveTmp, "."], { cwd: stage, stdio: "inherit" });
}

await rename(archiveTmp, output);
console.log(`created ${path.relative(root, output)}`);
