#!/usr/bin/env node
// Called only after every platform has built SOURCE_SHA with RELEASE_VERSION.
// Commits the gpui/ version bump and tags the tested source as gpui-vX.Y.Z —
// the "gpui-" prefix keeps the GPUI line disjoint from the kspkg vX.Y.Z tags.
//
// Required env: DEFAULT_BRANCH, SOURCE_SHA, RELEASE_VERSION.

import { execFileSync, spawnSync } from "node:child_process";

const { DEFAULT_BRANCH, SOURCE_SHA, RELEASE_VERSION } = process.env;
for (const [name, value] of Object.entries({ DEFAULT_BRANCH, SOURCE_SHA, RELEASE_VERSION })) {
  if (!value) {
    console.error(`${name} is not set`);
    process.exit(2);
  }
}

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

function gitStatus(...args) {
  const result = spawnSync("git", args, { stdio: "inherit" });
  if (result.error) throw result.error;
  return result.status;
}

function gitChecked(...args) {
  const status = gitStatus(...args);
  if (status !== 0) {
    throw new Error(`git ${args.join(" ")} failed with exit code ${status}`);
  }
}

function tryRev(spec) {
  const result = spawnSync("git", ["rev-parse", spec], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  return result.status === 0 ? result.stdout.trim() : null;
}

gitChecked("fetch", "origin", DEFAULT_BRANCH, "--tags");
execFileSync("node", ["scripts/release.mjs", "set", RELEASE_VERSION], { stdio: "inherit" });
git("config", "user.name", "github-actions[bot]");
git("config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com");
git("add", "gpui/Cargo.toml", "gpui/Cargo.lock");
const tag = `gpui-v${RELEASE_VERSION}`;
const remote = git("rev-parse", `origin/${DEFAULT_BRANCH}`);
let commit;
if (remote !== SOURCE_SHA) {
  // Permit only our own completed version commit when retrying publication.
  if (
    tryRev(`${remote}^`) !== SOURCE_SHA ||
    tryRev(`${remote}^{tree}`) !== git("write-tree") ||
    tryRev(`${tag}^{commit}`) !== remote
  ) {
    console.error("Default branch changed during the build. Rerun on the new commit.");
    process.exit(1);
  }
  commit = remote;
} else {
  if (gitStatus("diff", "--cached", "--quiet") !== 0) {
    gitChecked("-c", "core.hooksPath=/dev/null", "commit", "-m", `chore: release ${tag}`);
  }
  commit = git("rev-parse", "HEAD");
}
const tagExists =
  spawnSync("git", ["rev-parse", "--verify", `refs/tags/${tag}`], { stdio: "ignore" }).status ===
  0;
if (tagExists) {
  if (tryRev(`${tag}^{commit}`) !== commit) process.exit(1);
} else {
  git("tag", tag, commit);
}
gitChecked("push", "--atomic", "origin", `${commit}:refs/heads/${DEFAULT_BRANCH}`, `refs/tags/${tag}`);
