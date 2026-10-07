// Run with: node --test scripts/test_release.mjs

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { nextVersion, setVersion, versionTuple } from "./release.mjs";

const SCRIPTS = path.dirname(fileURLToPath(import.meta.url));
const RELEASE = path.join(SCRIPTS, "release.mjs");
const PUBLISH = path.join(SCRIPTS, "publish-version.mjs");

const MANIFEST =
  '[package]\nname = "dictation-gpui"\nversion = "0.1.0"\n\n[dependencies.example]\nversion = "0.1.0"\n';
const LOCK =
  'version = 4\n\n[[package]]\nname = "dictation-gpui"\nversion = "0.1.0"\n\n[[package]]\nname = "example"\nversion = "0.1.0"\n';

function mkdtemp() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "release-test-"));
}

// Git exports GIT_DIR/GIT_WORK_TREE/etc. while running hooks; without
// scrubbing, the temp repo's git calls would land in the caller's repository.
function cleanEnv(extra = {}) {
  const env = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (!key.startsWith("GIT_")) env[key] = value;
  }
  return { ...env, ...extra };
}

function writeExecutable(file, content) {
  fs.writeFileSync(file, content);
  fs.chmodSync(file, 0o755);
}

test("version rules", () => {
  assert.equal(nextVersion("0.1.0", null, true), "0.1.0");
  assert.equal(nextVersion("0.1.0", "0.1.0", true), "0.1.1");
  assert.equal(nextVersion("0.1.9", "0.1.9", true), "0.1.10");
  assert.equal(nextVersion("0.2.0", "0.1.9", true), "0.2.0");
  assert.equal(nextVersion("1.0.0", "0.2.9", true), "1.0.0");
  // Retry after a publication failure.
  assert.equal(nextVersion("0.1.1", "0.1.0", true), "0.1.1");
  assert.equal(nextVersion("0.1.1", "0.1.1", false), null);
  for (const invalid of ["1.2", "v1.2.3", "01.2.3", "1.2.3-rc.1", "1.2.3\n"]) {
    assert.throws(() => versionTuple(invalid), undefined, `Accepted invalid version ${invalid}`);
  }
  assert.throws(() => nextVersion("0.1.0", "0.2.0", true), undefined, "Accepted a version downgrade");
});

test("setVersion rewrites manifest and lock", () => {
  const directory = mkdtemp();
  const root = path.join(directory, "gpui");
  fs.mkdirSync(root);
  fs.writeFileSync(path.join(root, "Cargo.toml"), MANIFEST);
  fs.writeFileSync(path.join(root, "Cargo.lock"), LOCK);
  setVersion("1.0.0", root);
  const manifest = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  assert.match(manifest, /\[package\]\nname = "dictation-gpui"\nversion = "1\.0\.0"/);
  assert.match(manifest, /\[dependencies\.example\]\nversion = "0\.1\.0"/);
  const lock = fs.readFileSync(path.join(root, "Cargo.lock"), "utf8");
  assert.match(lock, /name = "dictation-gpui"\nversion = "1\.0\.0"/);
  assert.match(lock, /name = "example"\nversion = "0\.1\.0"/);
  setVersion("1.0.0", root); // Idempotent across matrix jobs and reruns.
  const before = fs.readFileSync(path.join(root, "Cargo.toml"));
  fs.writeFileSync(path.join(root, "Cargo.lock"), "version = 4\n");
  assert.throws(
    () => setVersion("2.0.0", root),
    undefined,
    "Accepted a missing application lock entry",
  );
  assert.deepEqual(fs.readFileSync(path.join(root, "Cargo.toml")), before);
});

// Exercise actual Git trees: no changes, empty commits, new work and manual bumps.
test("plan, publish and package against a git fixture", (t) => {
  const root = mkdtemp();
  const repo = path.join(root, "repo");
  fs.mkdirSync(repo);
  const fakeBin = path.join(root, "bin");
  fs.mkdirSync(fakeBin);
  // plan() shells out to `gh api --paginate --slurp`; the fake prints the
  // release pages supplied through FAKE_RELEASES.
  writeExecutable(path.join(fakeBin, "gh"), '#!/bin/sh\nprintf "%s" "$FAKE_RELEASES"\n');
  // codesign is exercised through a marker so tests run on any platform.
  const codesignMarker = path.join(root, "codesign-called");
  writeExecutable(
    path.join(fakeBin, "codesign"),
    `#!/bin/sh\ntouch "${codesignMarker}"\n`,
  );

  const git = (...args) =>
    execFileSync("git", args, {
      cwd: repo,
      env: cleanEnv(),
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();

  git("init");
  git("config", "user.name", "Release test");
  git("config", "user.email", "release-test@example.invalid");
  git("config", "core.hooksPath", path.join(root, "no-hooks"));
  git("config", "commit.gpgsign", "false");
  fs.mkdirSync(path.join(repo, "gpui"));
  fs.writeFileSync(path.join(repo, "gpui", "Cargo.toml"), MANIFEST);
  fs.writeFileSync(path.join(repo, "gpui", "Cargo.lock"), LOCK);
  git("add", ".");
  git("commit", "-m", "initial");

  const planned = (tags) => {
    const output = path.join(root, "output");
    fs.writeFileSync(output, "");
    const pages = [tags.map((tag) => ({ tag_name: tag, draft: false, prerelease: false }))];
    execFileSync("node", [RELEASE, "plan"], {
      cwd: repo,
      env: cleanEnv({
        GITHUB_OUTPUT: output,
        GITHUB_REPOSITORY: "test/repo",
        FAKE_RELEASES: JSON.stringify(pages),
        PATH: `${fakeBin}${path.delimiter}${process.env.PATH}`,
      }),
      stdio: ["ignore", "pipe", "inherit"],
    });
    return Object.fromEntries(
      fs
        .readFileSync(output, "utf8")
        .split("\n")
        .filter(Boolean)
        .map((line) => {
          const index = line.indexOf("=");
          return [line.slice(0, index), line.slice(index + 1)];
        }),
    );
  };

  assert.equal(planned([]).version, "0.1.0");
  git("tag", "gpui-v0.1.0");
  assert.equal(planned(["gpui-v0.1.0"]).release, "false");
  git("commit", "--allow-empty", "-m", "empty");
  assert.equal(planned(["gpui-v0.1.0"]).release, "false");
  // kspkg-side changes never release the GPUI app.
  fs.writeFileSync(path.join(repo, "kspkg.txt"), "vue only");
  git("add", ".");
  git("commit", "-m", "kspkg change");
  assert.equal(planned(["gpui-v0.1.0"]).release, "false");
  fs.writeFileSync(path.join(repo, "gpui", "change.txt"), "new work");
  git("add", ".");
  git("commit", "-m", "gpui change");
  let result = planned(["gpui-v0.1.0"]);
  assert.equal(result.version, "0.1.1");
  assert.equal(result.sha, git("rev-parse", "HEAD"));
  execFileSync("node", [RELEASE, "set", "1.0.0"], { cwd: repo, env: cleanEnv() });
  git("add", ".");
  git("commit", "-m", "manual major bump");
  assert.equal(planned(["gpui-v0.1.0"]).version, "1.0.0");
  git("tag", "gpui-v1.0.0");
  // A tag without a published release must retry the same version.
  assert.equal(planned(["gpui-v0.1.0"]).version, "1.0.0");
  assert.equal(planned(["gpui-v0.1.0", "gpui-v1.0.0"]).release, "false");
  // kspkg v-tags belong to a different line and are ignored.
  assert.equal(planned(["gpui-v0.1.0", "gpui-v1.0.0", "v0.2.5"]).release, "false");

  // Publish against a local bare remote, including retry and concurrent work.
  fs.mkdirSync(path.join(repo, "scripts"));
  fs.copyFileSync(RELEASE, path.join(repo, "scripts", "release.mjs"));
  git("add", ".");
  git("commit", "-m", "release script");
  const remote = path.join(root, "remote.git");
  git("init", "--bare", remote);
  git("remote", "add", "origin", remote);
  git("push", "origin", "HEAD:refs/heads/main", "--tags");
  const source = git("rev-parse", "HEAD");
  const environment = cleanEnv({
    DEFAULT_BRANCH: "main",
    SOURCE_SHA: source,
    RELEASE_VERSION: "1.0.1",
  });

  const publish = () =>
    spawnSync("node", [PUBLISH], {
      cwd: repo,
      env: environment,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });

  result = publish();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const released = git("rev-parse", "gpui-v1.0.1");
  assert.equal(git("rev-parse", "origin/main"), released);
  assert.equal(git("rev-parse", "HEAD^"), source);
  assert.match(
    fs.readFileSync(path.join(repo, "gpui", "Cargo.toml"), "utf8"),
    /version = "1\.0\.1"/,
  );
  git("checkout", "--detach", source);
  result = publish(); // Original SOURCE_SHA, as with GitHub's rerun-failed-jobs.
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.equal(git("rev-parse", "origin/main"), released);
  git("restore", "--staged", "--worktree", ".");
  git("checkout", "--detach", released);
  fs.writeFileSync(path.join(repo, "gpui", "change.txt"), "concurrent work");
  git("add", ".");
  git("commit", "-m", "concurrent work");
  const concurrent = git("rev-parse", "HEAD");
  git("push", "origin", "HEAD:refs/heads/main");
  git("checkout", "--detach", released);
  environment.SOURCE_SHA = released;
  environment.RELEASE_VERSION = "1.0.2";
  result = publish();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Default branch changed/);
  assert.equal(git("rev-parse", "origin/main"), concurrent);
  assert.equal(git("tag", "--list", "gpui-v1.0.2"), "");

  // Archives contain the right executable; the macOS bundle embeds the version.
  const packageEnv = cleanEnv({
    PATH: `${fakeBin}${path.delimiter}${process.env.PATH}`,
  });
  for (const target of [
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
  ]) {
    const filename = target.includes("windows") ? "dictation-gpui.exe" : "dictation-gpui";
    const binary = path.join(repo, "gpui", "target", target, "release", filename);
    fs.mkdirSync(path.dirname(binary), { recursive: true });
    fs.writeFileSync(binary, "test executable");
    fs.rmSync(codesignMarker, { force: true });
    execFileSync("node", [RELEASE, "package", target], {
      cwd: repo,
      env: packageEnv,
      stdio: ["ignore", "pipe", "inherit"],
    });
    assert.equal(fs.existsSync(codesignMarker), target.includes("apple"));
    const stem = `dist/dictation-gpui-1.0.2-${target}`;
    if (target.includes("windows")) {
      const zipped = execFileSync("unzip", ["-p", `${stem}.zip`, filename], {
        cwd: repo,
        encoding: "utf8",
      });
      assert.equal(zipped, "test executable");
    } else {
      const unpacked = path.join(root, `unpacked-${target}`);
      fs.mkdirSync(unpacked);
      execFileSync("tar", ["-xzf", `${stem}.tar.gz`, "-C", unpacked], { cwd: repo });
      const executable = target.includes("apple")
        ? "Dictation.app/Contents/MacOS/dictation-gpui"
        : filename;
      assert.equal(fs.readFileSync(path.join(unpacked, executable), "utf8"), "test executable");
      if (target.includes("apple")) {
        const plist = fs.readFileSync(
          path.join(unpacked, "Dictation.app/Contents/Info.plist"),
          "utf8",
        );
        assert.match(plist, /<key>CFBundleShortVersionString<\/key>\s*<string>1\.0\.2<\/string>/);
      }
    }
  }
});
