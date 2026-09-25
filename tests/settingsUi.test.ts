import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(path, "utf8");

describe("Dictation settings surface", () => {
  it("uses Imago chrome and a branded model selector", () => {
    const app = read("src/App.vue");
    const packageJson = JSON.parse(read("package.json"));

    expect(packageJson.dependencies["@kosmos/visuals"]).toBe("npm:@makekosmos/visuals@0.1.2");
    expect(app).toContain('<DesktopChrome appearance="settings" :platform="chromePlatform">');
    expect(app).toContain("document.documentElement.dataset.platform");
    expect(app).toContain('<SettingsSidebar title="Kosmos"');
    expect(app).toContain('<span class="dictation-titlebar-title">Dictation</span>');
    expect(app).toContain('title="Модель"');
    expect(app).toContain('@update:model-value="selectModel"');
    expect(app).toContain("через Kosmos Engine");
    expect(app).not.toContain("Kosmos Desktop.");
    expect(app).toContain("groqIconMarkup");
    expect(app).toContain("kosmosIconMarkup");
    expect(read("src/assets/providers/kosmos.svg")).toContain('fill="#fff"');
    expect(read("src/assets/providers/groq.svg")).toContain('fill="currentColor"');
  });
});
