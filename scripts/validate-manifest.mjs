import { readFile } from "node:fs/promises";

const manifest = JSON.parse(await readFile("manifest.json", "utf8"));
const required = [
  "schema_version",
  "id",
  "name",
  "version",
  "kind",
  "engine_api",
  "entrypoint",
  "icon",
  "publisher",
  "targets",
  "data",
];
const missing = required.filter((key) => !manifest[key]);
if (missing.length) throw new Error(`manifest.json: missing ${missing.join(", ")}`);
if (manifest.schema_version !== 2) throw new Error("manifest.json: expected schema_version 2");
if (manifest.id !== "com.kosmos.dictation") throw new Error(`manifest.json: unexpected id ${manifest.id}`);
if (manifest.kind !== "app") throw new Error("manifest.json: expected kind app");
if (manifest.entrypoint !== "dist/index.html") throw new Error("manifest.json: unexpected entrypoint");
if (manifest.icon !== "icon.png") throw new Error("manifest.json: expected PNG icon");
const requested = manifest.permissions.flatMap(({ capability, scopes = [] }) =>
  scopes.map((scope) => `${capability}:${scope}`),
);
const expected = [
  "ark.read:dictation.get_state",
  "ark.read:dictation.get_config",
  "ark.write:dictation.update_config",
  "ark.write:dictation.start_recording",
  "ark.write:dictation.cancel",
];
if (JSON.stringify(requested) !== JSON.stringify(expected))
  throw new Error("manifest.json: unexpected Dictation permissions");
console.log(`manifest ok: ${manifest.id} v${manifest.version}`);
