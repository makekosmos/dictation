import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, test } from "vitest";
import { createDeterministicZip, readCanonicalZip } from "../scripts/deterministic-zip.mjs";
import { collectPackageFiles } from "../scripts/package-kspkg.mjs";

describe("deterministic package archive", () => {
  test("preserves the published 0.2.3 fixture", () => {
    const fixture = readFileSync(path.join(import.meta.dirname, "fixtures/dictation-0.2.3.kspkg"));
    expect(createHash("sha256").update(fixture).digest("hex")).toBe(
      "30f42dadaf4d0412033f0fd63fbb116f3a8d4717e57e0bcba61d3930b019623f",
    );
  });

  test("pins the reviewed Package v2 fixture", () => {
    const fixture = readFileSync(path.join(import.meta.dirname, "fixtures/dictation-0.2.4.kspkg"));
    expect(createHash("sha256").update(fixture).digest("hex")).toBe(
      "a7eaf9c84ee63fc01799df531ba0469390a20c4a4df6e37f1c9901af8a4c5fd5",
    );
    const entries = new Map(readCanonicalZip(fixture).map((entry) => [entry.name, entry.data]));
    expect(JSON.parse(entries.get("manifest.json").toString("utf8"))).toMatchObject({
      id: "com.kosmos.dictation",
      version: "0.2.4",
    });
    expect(entries.get("compatibility.json")).toEqual(
      readFileSync(path.join(import.meta.dirname, "..", "compatibility.json")),
    );
  });

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
