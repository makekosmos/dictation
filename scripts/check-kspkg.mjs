import { existsSync, lstatSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { isDeepStrictEqual } from "node:util";
import { readCanonicalZip } from "./deterministic-zip.mjs";

const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
const manifest = JSON.parse(readFileSync("package.manifest.json", "utf8"));
const output = path.resolve(`release/dictation-${manifest.version}.kspkg`);
if (packageJson.version !== manifest.version) {
  throw new Error("package.json and package.manifest.json versions differ");
}
if (!existsSync(output)) throw new Error(`missing package: ${output}`);
if (readdirSync("release").some((name) => name.endsWith(".kext"))) {
  throw new Error("release contains an inactive .kext artifact");
}

function sourceEntries(source, archiveName) {
  const stat = lstatSync(source);
  if (stat.isSymbolicLink()) throw new Error(`package source cannot contain symlink: ${source}`);
  if (!stat.isDirectory()) return [{ name: archiveName, data: readFileSync(source) }];
  return [
    { name: `${archiveName}/`, data: Buffer.alloc(0), directory: true },
    ...readdirSync(source)
      .sort()
      .flatMap((entry) => sourceEntries(path.join(source, entry), `${archiveName}/${entry}`)),
  ];
}

const source = [
  ...sourceEntries("package.manifest.json", "manifest.json"),
  ...sourceEntries("compatibility.json", "compatibility.json"),
  ...sourceEntries("icon.png", "icon.png"),
  ...sourceEntries("dist", "dist"),
].sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0));
const archived = readCanonicalZip(readFileSync(output));
if (
  JSON.stringify(archived.map(({ name }) => name)) !==
  JSON.stringify(source.map(({ name }) => name))
) {
  throw new Error(`${output}: archive entries do not exactly match package source files`);
}
for (let index = 0; index < archived.length; index += 1) {
  if (!archived[index].data.equals(source[index].data)) {
    throw new Error(`${output}: stale archive entry ${archived[index].name}`);
  }
}
const archivedManifest = JSON.parse(
  archived.find(({ name }) => name === "manifest.json").data.toString("utf8"),
);
if (!isDeepStrictEqual(archivedManifest, manifest)) {
  throw new Error("archive manifest and package.manifest.json differ");
}
const tag = process.env.GITHUB_REF_NAME;
if (tag?.startsWith("v") && tag.slice(1) !== manifest.version) {
  throw new Error(
    `release tag and package.manifest.json versions differ: ${tag} !== v${manifest.version}`,
  );
}
console.log(`package ok: ${path.relative(process.cwd(), output)}`);
