"""Release versioning and archives for the GPUI app (Python 3.11+, standard library only).

The version source of truth is gpui/Cargo.toml. Release tags use the
"gpui-vX.Y.Z" prefix so the GPUI line cannot collide with the existing
kspkg "vX.Y.Z" releases (v0.2.x and earlier).
"""

import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tarfile
import tomllib
import zipfile

ROOT = Path("gpui")
TAG_PREFIX = "gpui-v"
APP = "dictation-gpui"
BUNDLE = "Dictation"


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def version_tuple(value):
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value):
        raise ValueError(f"Expected a stable MAJOR.MINOR.PATCH version, got {value!r}")
    return tuple(map(int, value.split(".")))


def next_version(current, previous, changed):
    current_parts = version_tuple(current)
    if previous is None:
        return current
    previous_parts = version_tuple(previous)
    if current_parts < previous_parts:
        raise ValueError(f"gpui/Cargo.toml version {current} is below released {previous}")
    if not changed:
        return None
    if current_parts > previous_parts:
        return current
    major, minor, patch = current_parts
    return f"{major}.{minor}.{patch + 1}"


def set_version(version, root=ROOT):
    version_tuple(version)
    manifest_path, lock_path = root / "Cargo.toml", root / "Cargo.lock"
    manifest, lock = manifest_path.read_text("utf-8"), lock_path.read_text("utf-8")
    package = tomllib.loads(manifest)["package"]["name"]
    manifest, count = re.subn(
        r'(?ms)(^\[package\]\s*.*?^version\s*=\s*")[^"]+("[^\n]*)',
        lambda match: match[1] + version + match[2], manifest, count=1,
    )
    if count != 1:
        raise ValueError("Cannot find package version in gpui/Cargo.toml")
    lock, count = re.subn(
        rf'(?m)(^name = "{re.escape(package)}"\nversion = ")[^"]+("[^\n]*)',
        lambda match: match[1] + version + match[2], lock,
    )
    if count != 1:
        raise ValueError("Expected exactly one application package in gpui/Cargo.lock")
    assert tomllib.loads(manifest)["package"]["version"] == version
    assert next(p for p in tomllib.loads(lock)["package"] if p["name"] == package)["version"] == version
    manifest_path.write_text(manifest, encoding="utf-8", newline="\n")
    lock_path.write_text(lock, encoding="utf-8", newline="\n")


def plan():
    # Published releases, not tags, are the baseline: a failed publication can retry.
    pages = json.loads(run(
        "gh", "api", "--paginate", "--slurp",
        f"repos/{os.environ['GITHUB_REPOSITORY']}/releases?per_page=100",
    ))
    tags = [
        release["tag_name"] for page in pages for release in page
        if not release["draft"] and not release["prerelease"]
        and re.fullmatch(
            re.escape(TAG_PREFIX) + r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)",
            release["tag_name"],
        )
    ]
    previous_tag = max(tags, key=lambda tag: version_tuple(tag[len(TAG_PREFIX):]), default=None)
    changed = True
    if previous_tag:
        # Missing tags, rewritten history and Git errors must fail, never look like no changes.
        subprocess.run(["git", "merge-base", "--is-ancestor", previous_tag, "HEAD"], check=True)
        # Only gpui/ changes ship in the GPUI artifact — kspkg edits alone
        # must not bump the app's release line.
        diff = subprocess.run(["git", "diff", "--quiet", previous_tag, "HEAD", "--", str(ROOT)])
        if diff.returncode not in (0, 1):
            diff.check_returncode()
        changed = diff.returncode == 1
    current = tomllib.loads((ROOT / "Cargo.toml").read_text("utf-8"))["package"]["version"]
    version = next_version(current, previous_tag[len(TAG_PREFIX):] if previous_tag else None, changed)
    values = {"version": version or "", "release": str(version is not None).lower(), "sha": run("git", "rev-parse", "HEAD"), "previous_tag": previous_tag or ""}
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for key, value in values.items():
            print(f"{key}={value}", file=output)
    print(f"Release {TAG_PREFIX}{version}" if version else "No gpui/ changes since the last release")


def package(target):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text("utf-8"))["package"]["version"]
    version_tuple(version)
    dist = Path("dist")
    dist.mkdir(exist_ok=True)
    name = f"{APP}-{version}-{target}"
    binary = ROOT / "target" / target / "release" / (f"{APP}.exe" if "windows" in target else APP)
    if "windows" in target:
        with zipfile.ZipFile(dist / f"{name}.zip", "w", zipfile.ZIP_DEFLATED) as archive:
            archive.write(binary, binary.name)
    elif "apple" in target:
        app = dist / f"{BUNDLE}.app"
        contents = app / "Contents"
        (contents / "MacOS").mkdir(parents=True, exist_ok=True)
        executable = contents / "MacOS" / APP
        shutil.copy2(binary, executable)
        executable.chmod(0o755)
        with (contents / "Info.plist").open("wb") as plist:
            plistlib.dump({
                "CFBundleName": BUNDLE, "CFBundleDisplayName": BUNDLE,
                "CFBundleIdentifier": f"com.makekosmos.{APP}",
                "CFBundleExecutable": APP, "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": version, "CFBundleVersion": version,
                "NSHighResolutionCapable": True,
            }, plist)
        subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(app)], check=True)
        with tarfile.open(dist / f"{name}.tar.gz", "w:gz") as archive:
            archive.add(app, arcname=app.name)
    else:
        with tarfile.open(dist / f"{name}.tar.gz", "w:gz") as archive:
            archive.add(binary, arcname=binary.name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("plan")
    commands.add_parser("set").add_argument("version")
    commands.add_parser("package").add_argument("target", choices=[
        "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu", "aarch64-apple-darwin",
    ])
    args = parser.parse_args()
    if args.command == "plan":
        plan()
    elif args.command == "set":
        set_version(args.version)
    else:
        package(args.target)
