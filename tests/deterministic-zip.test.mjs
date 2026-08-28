import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, test } from "vitest";
import { createDeterministicZip, readCanonicalZip } from "../scripts/deterministic-zip.mjs";
import { collectPackageFiles } from "../scripts/package-kspkg.mjs";

describe("deterministic package archive", () => {
  test("is order-independent and rejects traversal", () => {
    const files = [
      { name: "manifest.json", data: Buffer.from("{}") },
      { name: "dist/", data: Buffer.alloc(0), directory: true },
      { name: "dist/index.html", data: Buffer.from("<main></main>") },
    ];
    const archive = createDeterministicZip(files);
    expect(archive).toEqual(createDeterministicZip([...files].reverse()));
    expect(readCanonicalZip(archive).map(({ name }) => name)).toEqual([
      "dist/",
      "dist/index.html",
      "manifest.json",
    ]);
    for (const name of ["../escape", "/absolute", "safe\\..\\escape", "a//b", "./dot", "nul\0x"])
      expect(() => createDeterministicZip([{ name, data: Buffer.alloc(0) }])).toThrow(
        "unsafe ZIP entry",
      );
    expect(() =>
      createDeterministicZip([
        { name: "same", data: Buffer.alloc(0) },
        { name: "same", data: Buffer.alloc(0) },
      ]),
    ).toThrow("duplicate ZIP entry");
    archive[archive.length - 1] ^= 1;
    expect(() => readCanonicalZip(archive)).toThrow("not deterministic canonical output");
  });

  test("rejects a symlink in the package source tree", () => {
    const root = mkdtempSync(path.join(os.tmpdir(), "dictation-package-test-"));
    try {
      for (const file of ["package.manifest.json", "compatibility.json", "icon.png"])
        writeFileSync(path.join(root, file), "{}");
      const dist = path.join(root, "dist");
      const target = path.join(root, "target");
      mkdirSync(dist);
      mkdirSync(target);
      symlinkSync(
        target,
        path.join(dist, "linked"),
        process.platform === "win32" ? "junction" : "dir",
      );
      expect(() => collectPackageFiles(root)).toThrow("package source cannot contain symlink");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
