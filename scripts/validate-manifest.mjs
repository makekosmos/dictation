import { readFile } from "node:fs/promises";

const manifest = JSON.parse(await readFile("manifest.json", "utf8"));
const required = ["id", "name", "version", "kind", "entryHtml", "icon"];
const missing = required.filter((key) => !manifest[key]);
if (missing.length) throw new Error(`manifest.json: missing ${missing.join(", ")}`);
if (manifest.id !== "dictation") throw new Error(`manifest.json: unexpected id ${manifest.id}`);
if (manifest.kind !== "vue") throw new Error("manifest.json: expected kind vue");
if (JSON.stringify(manifest.permissions) !== JSON.stringify(["dictation.control"])) {
  throw new Error("manifest.json: Dictation may only request dictation.control");
}
console.log(`manifest ok: ${manifest.id} v${manifest.version}`);
