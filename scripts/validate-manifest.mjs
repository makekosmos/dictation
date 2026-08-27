import { existsSync, readFileSync } from "node:fs";

const packageManifest = JSON.parse(readFileSync("package.manifest.json", "utf8"));
const legacyManifest = JSON.parse(readFileSync("manifest.json", "utf8"));
const compatibility = JSON.parse(readFileSync("compatibility.json", "utf8"));
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
const missing = required.filter((key) => !packageManifest[key]);
if (missing.length) throw new Error(`package.manifest.json: missing ${missing.join(", ")}`);
if (packageManifest.schema_version !== 2)
  throw new Error("package.manifest.json: expected schema_version 2");
if (packageManifest.id !== "com.kosmos.dictation")
  throw new Error(`package.manifest.json: unexpected id ${packageManifest.id}`);
if (
  packageManifest.version !== packageJson.version ||
  legacyManifest.version !== packageJson.version
)
  throw new Error("package.json, package.manifest.json and manifest.json versions differ");
if (packageManifest.kind !== "app") throw new Error("package.manifest.json: expected kind app");
if (packageManifest.entrypoint !== "dist/index.html")
  throw new Error("package.manifest.json: unexpected entrypoint");
if (packageManifest.icon !== "icon.png")
  throw new Error("package.manifest.json: expected PNG icon");
if (!existsSync(packageManifest.icon))
  throw new Error(`package.manifest.json: missing referenced file ${packageManifest.icon}`);
if (legacyManifest.id !== "dictation")
  throw new Error(`manifest.json: unexpected legacy id ${legacyManifest.id}`);
if (!legacyManifest.icon || !existsSync(legacyManifest.icon))
  throw new Error(`manifest.json: missing legacy referenced file ${legacyManifest.icon}`);
if (JSON.stringify(legacyManifest.permissions) !== JSON.stringify(["dictation.control"]))
  throw new Error("manifest.json: unexpected legacy Dictation permissions");
if (
  compatibility.current.app_id !== packageManifest.id ||
  compatibility.current.extension_id !== legacyManifest.id
)
  throw new Error("compatibility.json does not point to the current Dictation identity");
if (compatibility.legacy.remove_after !== "1.0.0")
  throw new Error("compatibility.json: bounded removal version is required");
if (JSON.stringify(compatibility.legacy.permissions) !== JSON.stringify(["dictation.control"]))
  throw new Error("compatibility.json: legacy permission must be preserved");
if (
  !["config", "credentials", "models", "tools", "permissions"].every((item) =>
    compatibility.legacy.data?.preserve?.includes(item),
  )
)
  throw new Error("compatibility.json: Dictation data preservation is incomplete");
const requested = packageManifest.permissions.flatMap(({ capability, scopes = [] }) =>
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
  throw new Error("package.manifest.json: unexpected Dictation permissions");
console.log(`manifest ok: ${packageManifest.id} v${packageManifest.version}`);
