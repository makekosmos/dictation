import { existsSync, readFileSync } from "node:fs";

const manifest = JSON.parse(readFileSync("manifest.json", "utf8"));
const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
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
if (manifest.id !== "com.kosmos.dictation")
  throw new Error(`manifest.json: unexpected id ${manifest.id}`);
if (manifest.version !== packageJson.version)
  throw new Error(
    `manifest.json and package.json versions differ: ${manifest.version} !== ${packageJson.version}`,
  );
if (manifest.kind !== "app") throw new Error("manifest.json: expected kind app");
if (manifest.entrypoint !== "dist/index.html")
  throw new Error("manifest.json: unexpected entrypoint");
if (manifest.icon !== "icon.png") throw new Error("manifest.json: expected PNG icon");
if (!existsSync(manifest.icon))
  throw new Error(`manifest.json: missing referenced file ${manifest.icon}`);
const requested = manifest.permissions.flatMap(({ capability, scopes = [] }) =>
  scopes.map((scope) => `${capability}:${scope}`),
);
const expected = [
  "ark.read:dictation.get_state",
  "ark.read:dictation.get_config",
  "ark.read:dictation.list_local_models",
  "ark.write:dictation.update_config",
  "ark.write:dictation.start_recording",
  "ark.write:dictation.cancel",
];
if (JSON.stringify(requested) !== JSON.stringify(expected))
  throw new Error("manifest.json: unexpected Dictation permissions");
console.log(`manifest ok: ${manifest.id} v${manifest.version}`);
