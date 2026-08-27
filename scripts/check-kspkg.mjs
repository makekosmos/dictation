import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const packageJson = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
const manifest = JSON.parse(readFileSync(path.join(root, "package.manifest.json"), "utf8"));
const output = path.join(root, `release/dictation-${manifest.version}.kspkg`);

if (packageJson.version !== manifest.version)
  throw new Error("package.json and package.manifest.json versions differ");
if (!existsSync(output)) throw new Error(`missing package: ${output}`);
if (readdirSync(path.join(root, "release")).some((name) => name.endsWith(".kext"))) {
  throw new Error("release contains an inactive .kext artifact");
}

const archiveArg = path.relative(root, output).replaceAll(path.sep, "/");
const windows = process.platform === "win32";
const contents = execFileSync(
  windows ? "tar" : "unzip",
  windows ? ["-tf", archiveArg] : ["-Z1", archiveArg],
  {
    cwd: root,
    encoding: "utf8",
  },
);
const entries = contents.split(/\r?\n/).filter(Boolean);
for (const entry of [
  "manifest.json",
  "compatibility.json",
  "icon.png",
  "dist/",
  "dist/index.html",
]) {
  if (!entries.includes(entry)) throw new Error(`${output}: missing ${entry}`);
}

const archivedManifest = JSON.parse(
  execFileSync(
    windows ? "tar" : "unzip",
    windows ? ["-xOf", archiveArg, "manifest.json"] : ["-p", archiveArg, "manifest.json"],
    {
      cwd: root,
      encoding: "utf8",
    },
  ),
);
if (archivedManifest.id !== "com.kosmos.dictation")
  throw new Error(`package id mismatch: ${archivedManifest.id}`);
if (archivedManifest.version !== manifest.version)
  throw new Error("archive manifest and package.manifest.json versions differ");

const tag = process.env.GITHUB_REF_NAME;
if (tag?.startsWith("v") && tag.slice(1) !== manifest.version) {
  throw new Error(
    `release tag and package.manifest.json versions differ: ${tag} !== v${manifest.version}`,
  );
}
console.log(`package ok: ${path.relative(root, output)}`);
