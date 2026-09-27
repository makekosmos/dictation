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

  it("refuses to save settings the app never loaded", () => {
    // If get_config fails the form keeps the hardcoded fallback — saving it
    // would overwrite the user's real Engine config with defaults.
    const app = read("src/App.vue");
    expect(app).toContain("settingsLoaded");
    expect(app).toContain(':disabled="!settingsLoaded"');
    expect(app).toMatch(/if \(!settingsLoaded\.value\)/);
  });

  it("keeps a failed local-model list from failing the whole load", () => {
    // list_local_models is advisory: on an Engine that lacks or fails the op
    // the config still loaded, so the load error must not claim failure and
    // the dropdown simply shows no local options.
    const app = read("src/App.vue");
    const load = app.slice(app.indexOf("async function load"));
    const between = load.slice(
      load.indexOf("settingsLoaded.value = true"),
      load.indexOf("listLocalModels"),
    );
    // The get_config try/catch must be closed before the models fetch —
    // sharing one try meant a models failure printed a bogus
    // "Не удалось загрузить настройки" over an actually-loaded form.
    expect(between).toContain("catch");
  });
});
