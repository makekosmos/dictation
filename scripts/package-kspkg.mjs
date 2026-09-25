import { lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createDeterministicZip } from "./deterministic-zip.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export function collectPackageFiles(packageRoot) {
  const files = [];
  function addFile(source, archiveName) {
    const stat = lstatSync(source);
    if (stat.isSymbolicLink()) throw new Error(`package source cannot contain symlink: ${source}`);
    if (!stat.isFile()) throw new Error(`package source must be a regular file: ${source}`);
    files.push({ name: archiveName, data: readFileSync(source) });
  }

  function addTree(source, archiveName) {
    const stat = lstatSync(source);
    if (stat.isSymbolicLink()) throw new Error(`package source cannot contain symlink: ${source}`);
    if (!stat.isDirectory()) throw new Error(`package source must be a directory: ${source}`);
    files.push({ name: `${archiveName}/`, data: Buffer.alloc(0), directory: true });
    for (const entry of readdirSync(source).sort()) {
      const childSource = path.join(source, entry);
      const childName = `${archiveName}/${entry}`;
      const childStat = lstatSync(childSource);
      if (childStat.isDirectory()) addTree(childSource, childName);
      else addFile(childSource, childName);
    }
  }
  addFile(path.join(packageRoot, "package.manifest.json"), "manifest.json");
  addFile(path.join(packageRoot, "compatibility.json"), "compatibility.json");
  addFile(path.join(packageRoot, "icon.png"), "icon.png");
  addTree(path.join(packageRoot, "dist"), "dist");
  addFile(path.join(packageRoot, "worker", "dictation-worker.exe"), "worker/dictation-worker.exe");
  return files;
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]).toLowerCase() === fileURLToPath(import.meta.url).toLowerCase()
) {
  const manifest = JSON.parse(readFileSync(path.join(root, "package.manifest.json"), "utf8"));
  const releaseDir = path.join(root, "release");
  const output = path.join(releaseDir, `dictation-${manifest.version}.kspkg`);
  mkdirSync(releaseDir, { recursive: true });
  rmSync(output, { force: true });
  writeFileSync(output, createDeterministicZip(collectPackageFiles(root)));
  console.log(`created ${path.relative(root, output)}`);
}
