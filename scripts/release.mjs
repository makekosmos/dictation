#!/usr/bin/env node
// Release versioning and archives for the GPUI app (Node.js, standard library only).
//
// The version source of truth is gpui/Cargo.toml. Release tags use the
// "gpui-vX.Y.Z" prefix so the GPUI line cannot collide with the existing
// kspkg "vX.Y.Z" releases (v0.2.x and earlier).
//
// Usage: node scripts/release.mjs plan | set <version> | package <target>

import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const ROOT = "gpui";
const TAG_PREFIX = "gpui-v";
const APP = "dictation-gpui";
const BUNDLE = "Dictation";
const VERSION_RE = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/;
const TARGETS = [
  "x86_64-pc-windows-msvc",
  "x86_64-unknown-linux-gnu",
  "aarch64-apple-darwin",
];

function run(...args) {
  return execFileSync(args[0], args.slice(1), { encoding: "utf8" }).trim();
}

function checked(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
}

export function versionTuple(value) {
  if (!VERSION_RE.test(value)) {
    throw new Error(`Expected a stable MAJOR.MINOR.PATCH version, got ${JSON.stringify(value)}`);
  }
  return value.split(".").map(Number);
}

function compareTuples(a, b) {
  for (let i = 0; i < 3; i += 1) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return 0;
}

function escapeRe(text) {
  return text.replace(/[^a-zA-Z0-9]/g, (ch) => `\\${ch}`);
}

// Minimal TOML access: the script only ever needs fields from the [package]
// section of gpui/Cargo.toml and name/version pairs inside gpui/Cargo.lock.
function packageSection(text) {
  const head = /^\[package\]/m.exec(text);
  if (!head) throw new Error("Cannot find [package] section in gpui/Cargo.toml");
  const rest = text.slice(head.index + head[0].length);
  const next = /^\[/m.exec(rest);
  return next ? rest.slice(0, next.index) : rest;
}

function manifestField(text, field) {
  const match = new RegExp(`^${field}\\s*=\\s*"([^"]*)"`, "m").exec(packageSection(text));
  if (!match) throw new Error(`Cannot find "${field}" in the [package] section`);
  return match[1];
}

function readVersion(root = ROOT) {
  return manifestField(fs.readFileSync(path.join(root, "Cargo.toml"), "utf8"), "version");
}

export function nextVersion(current, previous, changed) {
  const currentParts = versionTuple(current);
  if (previous == null) return current;
  const previousParts = versionTuple(previous);
  if (compareTuples(currentParts, previousParts) < 0) {
    throw new Error(`gpui/Cargo.toml version ${current} is below released ${previous}`);
  }
  if (!changed) return null;
  if (compareTuples(currentParts, previousParts) > 0) return current;
  const [major, minor, patch] = currentParts;
  return `${major}.${minor}.${patch + 1}`;
}

export function setVersion(version, root = ROOT) {
  versionTuple(version);
  const manifestPath = path.join(root, "Cargo.toml");
  const lockPath = path.join(root, "Cargo.lock");
  let manifest = fs.readFileSync(manifestPath, "utf8");
  let lock = fs.readFileSync(lockPath, "utf8");
  const pkg = manifestField(manifest, "name");
  let count = 0;
  manifest = manifest.replace(
    /(^\[package\][\s\S]*?^version\s*=\s*")[^"]+("[^\n]*)/m,
    (match, head, tail) => {
      count += 1;
      return head + version + tail;
    },
  );
  if (count !== 1) throw new Error("Cannot find package version in gpui/Cargo.toml");
  count = 0;
  lock = lock.replace(
    new RegExp(`(^name = "${escapeRe(pkg)}"\\nversion = ")[^"]+("[^\\n]*)`, "gm"),
    (match, head, tail) => {
      count += 1;
      return head + version + tail;
    },
  );
  if (count !== 1) {
    throw new Error("Expected exactly one application package in gpui/Cargo.lock");
  }
  if (manifestField(manifest, "version") !== version) {
    throw new Error(`Manifest version check failed after rewriting to ${version}`);
  }
  const lockEntry = new RegExp(`^name = "${escapeRe(pkg)}"\\nversion = "([^"]*)"`, "m").exec(lock);
  if (!lockEntry || lockEntry[1] !== version) {
    throw new Error(`Lockfile version check failed after rewriting to ${version}`);
  }
  fs.writeFileSync(manifestPath, manifest);
  fs.writeFileSync(lockPath, lock);
}

export function plan() {
  // Published releases, not tags, are the baseline: a failed publication can retry.
  const pages = JSON.parse(
    run(
      "gh",
      "api",
      "--paginate",
      "--slurp",
      `repos/${process.env.GITHUB_REPOSITORY}/releases?per_page=100`,
    ),
  );
  const tagRe = new RegExp(
    `^${escapeRe(TAG_PREFIX)}${VERSION_RE.source.slice(1, -1)}$`,
  );
  const tags = [];
  for (const page of pages) {
    for (const release of page) {
      if (!release.draft && !release.prerelease && tagRe.test(release.tag_name)) {
        tags.push(release.tag_name);
      }
    }
  }
  let previousTag = null;
  for (const tag of tags) {
    if (
      previousTag === null ||
      compareTuples(
        versionTuple(tag.slice(TAG_PREFIX.length)),
        versionTuple(previousTag.slice(TAG_PREFIX.length)),
      ) > 0
    ) {
      previousTag = tag;
    }
  }
  let changed = true;
  if (previousTag) {
    // Missing tags, rewritten history and Git errors must fail, never look like no changes.
    checked("git", ["merge-base", "--is-ancestor", previousTag, "HEAD"]);
    // Only gpui/ changes ship in the GPUI artifact — kspkg edits alone
    // must not bump the app's release line.
    const diff = spawnSync("git", ["diff", "--quiet", previousTag, "HEAD", "--", ROOT]);
    if (diff.error) throw diff.error;
    if (diff.status !== 0 && diff.status !== 1) {
      throw new Error(`git diff failed with exit code ${diff.status}`);
    }
    changed = diff.status === 1;
  }
  const current = readVersion();
  const version = nextVersion(
    current,
    previousTag ? previousTag.slice(TAG_PREFIX.length) : null,
    changed,
  );
  const values = {
    version: version ?? "",
    release: String(version !== null),
    sha: run("git", "rev-parse", "HEAD"),
    previous_tag: previousTag ?? "",
  };
  fs.appendFileSync(
    process.env.GITHUB_OUTPUT,
    Object.entries(values)
      .map(([key, value]) => `${key}=${value}`)
      .join("\n") + "\n",
  );
  console.log(
    version ? `Release ${TAG_PREFIX}${version}` : "No gpui/ changes since the last release",
  );
}

function infoPlist(version) {
  const entries = [
    ["CFBundleDisplayName", "string", BUNDLE],
    ["CFBundleExecutable", "string", APP],
    ["CFBundleIdentifier", "string", `com.makekosmos.${APP}`],
    ["CFBundleName", "string", BUNDLE],
    ["CFBundlePackageType", "string", "APPL"],
    ["CFBundleShortVersionString", "string", version],
    ["CFBundleVersion", "string", version],
    ["NSHighResolutionCapable", "true", null],
  ];
  const body = entries
    .map(([key, type, value]) =>
      type === "true"
        ? `\t<key>${key}</key>\n\t<true/>`
        : `\t<key>${key}</key>\n\t<${type}>${value}</${type}>`,
    )
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
${body}
</dict>
</plist>
`;
}

export function packageTarget(target) {
  const version = readVersion();
  versionTuple(version);
  const dist = "dist";
  fs.mkdirSync(dist, { recursive: true });
  const name = `${APP}-${version}-${target}`;
  const binary = path.join(
    ROOT,
    "target",
    target,
    "release",
    target.includes("windows") ? `${APP}.exe` : APP,
  );
  if (target.includes("windows")) {
    checked("zip", ["-j", "-q", path.join(dist, `${name}.zip`), binary]);
  } else if (target.includes("apple")) {
    const app = path.join(dist, `${BUNDLE}.app`);
    const contents = path.join(app, "Contents");
    fs.mkdirSync(path.join(contents, "MacOS"), { recursive: true });
    const executable = path.join(contents, "MacOS", APP);
    fs.copyFileSync(binary, executable);
    fs.chmodSync(executable, 0o755);
    fs.writeFileSync(path.join(contents, "Info.plist"), infoPlist(version));
    checked("codesign", ["--force", "--deep", "--sign", "-", app]);
    checked("tar", ["-czf", path.join(dist, `${name}.tar.gz`), "-C", dist, `${BUNDLE}.app`]);
  } else {
    checked("tar", [
      "-czf",
      path.join(dist, `${name}.tar.gz`),
      "-C",
      path.dirname(binary),
      path.basename(binary),
    ]);
  }
}

function usage() {
  console.error("Usage: node scripts/release.mjs plan | set <version> | package <target>");
  process.exit(2);
}

const invokedAsScript =
  process.argv[1] && import.meta.url === pathToFileURL(fs.realpathSync(process.argv[1])).href;
if (invokedAsScript) {
  const [command, arg] = process.argv.slice(2);
  if (command === "plan" && arg === undefined) {
    plan();
  } else if (command === "set" && arg !== undefined) {
    setVersion(arg);
  } else if (command === "package" && TARGETS.includes(arg)) {
    packageTarget(arg);
  } else {
    usage();
  }
}
