import { existsSync, readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
const commandFiles = ["package.json", "README.md", "lefthook.yml", ".github/workflows/ci.yml"];

describe("package manager migration", () => {
  test("uses pnpm for package management and orchestration", () => {
    expect(packageJson.packageManager).toBe("pnpm@12.4.1");
    expect(existsSync("pnpm-lock.yaml")).toBe(true);
    expect(existsSync("bun.lock")).toBe(false);
    for (const file of commandFiles) {
      expect(readFileSync(file, "utf8")).not.toMatch(/\bbun (?:install|run|test|audit)\b/);
      expect(readFileSync(file, "utf8")).not.toMatch(/setup-bun/);
    }
    expect(packageJson.devDependencies["bun-types"]).toBeUndefined();
  });
});
