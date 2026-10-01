"""Run with: python scripts/test_release.py"""

from pathlib import Path
import json
import os
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from unittest.mock import patch
import zipfile

import release
from release import next_version, set_version, version_tuple


def test_release():
    scripts = Path(__file__).resolve().parent
    assert next_version("0.1.0", None, True) == "0.1.0"
    assert next_version("0.1.0", "0.1.0", True) == "0.1.1"
    assert next_version("0.1.9", "0.1.9", True) == "0.1.10"
    assert next_version("0.2.0", "0.1.9", True) == "0.2.0"
    assert next_version("1.0.0", "0.2.9", True) == "1.0.0"
    assert next_version("0.1.1", "0.1.0", True) == "0.1.1"  # Retry after a publication failure.
    assert next_version("0.1.1", "0.1.1", False) is None
    for invalid in ["1.2", "v1.2.3", "01.2.3", "1.2.3-rc.1", "1.2.3\n"]:
        try:
            version_tuple(invalid)
        except ValueError:
            pass
        else:
            raise AssertionError(f"Accepted invalid version {invalid!r}")
    try:
        next_version("0.1.0", "0.2.0", True)
    except ValueError:
        pass
    else:
        raise AssertionError("Accepted a version downgrade")

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / "gpui"
        root.mkdir()
        manifest = '[package]\nname = "dictation-gpui"\nversion = "0.1.0"\n\n[dependencies.example]\nversion = "0.1.0"\n'
        lock = 'version = 4\n\n[[package]]\nname = "dictation-gpui"\nversion = "0.1.0"\n\n[[package]]\nname = "example"\nversion = "0.1.0"\n'
        (root / "Cargo.toml").write_text(manifest, encoding="utf-8")
        (root / "Cargo.lock").write_text(lock, encoding="utf-8")
        set_version("1.0.0", root)
        parsed = tomllib.loads((root / "Cargo.toml").read_text("utf-8"))
        assert parsed["package"]["version"] == "1.0.0"
        assert parsed["dependencies"]["example"]["version"] == "0.1.0"
        packages = tomllib.loads((root / "Cargo.lock").read_text("utf-8"))["package"]
        assert [p["version"] for p in packages] == ["1.0.0", "0.1.0"]
        set_version("1.0.0", root)  # Idempotent across matrix jobs and reruns.
        before = (root / "Cargo.toml").read_bytes()
        (root / "Cargo.lock").write_text('version = 4\n', encoding="utf-8")
        try:
            set_version("2.0.0", root)
        except ValueError:
            pass
        else:
            raise AssertionError("Accepted a missing application lock entry")
        assert (root / "Cargo.toml").read_bytes() == before

    # Exercise actual Git trees: no changes, empty commits, new work and manual bumps.
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        repo = root / "repo"
        repo.mkdir()
        previous_cwd = Path.cwd()
        # Git exports GIT_DIR/GIT_WORK_TREE/etc. while running hooks; without
        # scrubbing, the temp repo's git/config calls would land in the
        # caller's repository. release.run() shares this process's env, so
        # strip them process-wide for the duration of the fixture.
        git_env = {k: os.environ.pop(k) for k in list(os.environ) if k.startswith("GIT_")}
        os.chdir(repo)
        try:
            def git(*args):
                return subprocess.check_output(["git", *args], text=True, stderr=subprocess.PIPE).strip()

            git("init")
            git("config", "user.name", "Release test")
            git("config", "user.email", "release-test@example.invalid")
            git("config", "core.hooksPath", str(root / "no-hooks"))
            git("config", "commit.gpgsign", "false")
            Path("gpui").mkdir()
            Path("gpui/Cargo.toml").write_text(manifest, encoding="utf-8")
            Path("gpui/Cargo.lock").write_text(lock, encoding="utf-8")
            git("add", ".")
            git("commit", "-m", "initial")
            real_run = release.run

            def planned(tags):
                output = root / "output"
                output.write_text("", encoding="utf-8")
                pages = [[{"tag_name": tag, "draft": False, "prerelease": False} for tag in tags]]
                with patch.dict(os.environ, {"GITHUB_OUTPUT": str(output), "GITHUB_REPOSITORY": "test/repo"}), patch.object(
                    release, "run", side_effect=lambda *args: json.dumps(pages) if args[0] == "gh" else real_run(*args)
                ):
                    release.plan()
                return dict(line.split("=", 1) for line in output.read_text("utf-8").splitlines())

            assert planned([])["version"] == "0.1.0"
            git("tag", "gpui-v0.1.0")
            assert planned(["gpui-v0.1.0"])["release"] == "false"
            git("commit", "--allow-empty", "-m", "empty")
            assert planned(["gpui-v0.1.0"])["release"] == "false"
            # kspkg-side changes never release the GPUI app.
            Path("kspkg.txt").write_text("vue only", encoding="utf-8")
            git("add", ".")
            git("commit", "-m", "kspkg change")
            assert planned(["gpui-v0.1.0"])["release"] == "false"
            Path("gpui/change.txt").write_text("new work", encoding="utf-8")
            git("add", ".")
            git("commit", "-m", "gpui change")
            result = planned(["gpui-v0.1.0"])
            assert result["version"] == "0.1.1"
            assert result["sha"] == git("rev-parse", "HEAD")
            set_version("1.0.0")
            git("add", ".")
            git("commit", "-m", "manual major bump")
            assert planned(["gpui-v0.1.0"])["version"] == "1.0.0"
            git("tag", "gpui-v1.0.0")
            # A tag without a published release must retry the same version.
            assert planned(["gpui-v0.1.0"])["version"] == "1.0.0"
            assert planned(["gpui-v0.1.0", "gpui-v1.0.0"])["release"] == "false"
            # kspkg v-tags belong to a different line and are ignored.
            assert planned(["gpui-v0.1.0", "gpui-v1.0.0", "v0.2.5"])["release"] == "false"

            # Publish against a local bare remote, including retry and concurrent work.
            Path("scripts").mkdir()
            shutil.copy2(scripts / "release.py", "scripts/release.py")
            git("add", ".")
            git("commit", "-m", "release script")
            remote = root / "remote.git"
            git("init", "--bare", str(remote))
            git("remote", "add", "origin", str(remote))
            git("push", "origin", "HEAD:refs/heads/main", "--tags")
            source = git("rev-parse", "HEAD")
            bash = shutil.which("bash") or "C:/Program Files/Git/bin/bash.exe"
            environment = dict(os.environ, DEFAULT_BRANCH="main", SOURCE_SHA=source, RELEASE_VERSION="1.0.1")

            def publish():
                return subprocess.run(
                    [bash, str(scripts / "publish-version.sh")], env=environment,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )

            result = publish()
            assert result.returncode == 0, result.stdout + result.stderr
            released = git("rev-parse", "gpui-v1.0.1")
            assert git("rev-parse", "origin/main") == released
            assert git("rev-parse", "HEAD^") == source
            assert tomllib.loads(Path("gpui/Cargo.toml").read_text("utf-8"))["package"]["version"] == "1.0.1"
            git("checkout", "--detach", source)
            result = publish()  # Original SOURCE_SHA, as with GitHub's rerun-failed-jobs.
            assert result.returncode == 0, result.stdout + result.stderr
            assert git("rev-parse", "origin/main") == released
            git("restore", "--staged", "--worktree", ".")
            git("checkout", "--detach", released)
            Path("gpui/change.txt").write_text("concurrent work", encoding="utf-8")
            git("add", ".")
            git("commit", "-m", "concurrent work")
            concurrent = git("rev-parse", "HEAD")
            git("push", "origin", "HEAD:refs/heads/main")
            git("checkout", "--detach", released)
            environment.update(SOURCE_SHA=released, RELEASE_VERSION="1.0.2")
            result = publish()
            assert result.returncode != 0
            assert "Default branch changed" in result.stderr
            assert git("rev-parse", "origin/main") == concurrent
            assert not git("tag", "--list", "gpui-v1.0.2")

            # Archives contain the right executable; the macOS bundle embeds the version.
            for target in ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu", "aarch64-apple-darwin"]:
                filename = "dictation-gpui.exe" if "windows" in target else "dictation-gpui"
                binary = Path("gpui/target") / target / "release" / filename
                binary.parent.mkdir(parents=True)
                binary.write_bytes(b"test executable")
                with patch.object(release.subprocess, "run") as signer:
                    release.package(target)
                    assert signer.called == ("apple" in target)
                stem = f"dist/dictation-gpui-1.0.2-{target}"
                if "windows" in target:
                    with zipfile.ZipFile(stem + ".zip") as archive:
                        assert archive.read(filename) == b"test executable"
                else:
                    with tarfile.open(stem + ".tar.gz") as archive:
                        executable = "Dictation.app/Contents/MacOS/dictation-gpui" if "apple" in target else filename
                        assert archive.extractfile(executable).read() == b"test executable"
                        if "apple" in target:
                            plist = plistlib.load(archive.extractfile("Dictation.app/Contents/Info.plist"))
                            assert plist["CFBundleShortVersionString"] == "1.0.2"
        finally:
            os.chdir(previous_cwd)
            os.environ.update(git_env)
    print("Release version checks passed")


if __name__ == "__main__":
    test_release()
