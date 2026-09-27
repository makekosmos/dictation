import { describe, expect, it } from "vitest";
import { buildAccelerator } from "../src/settings/composables/useDictationConfig.shared";

describe("hotkey accelerator", () => {
  it("names VK_OEM_PLUS 'Plus' — '+' is the accelerator delimiter", () => {
    // 0xBB is the =/+ key. Mapping it to the literal "+" produced "Ctrl++":
    // the parser's delimiter swallows the key part, so the captured hotkey
    // round-trips as a bare modifier and the pill footer shows only "Ctrl".
    const accel = buildAccelerator({ vk: 0xbb, ctrl: true });
    expect(accel).toBe("Ctrl+Plus");
    expect(accel.split("+").filter(Boolean)).toEqual(["Ctrl", "Plus"]);
  });

  it("keeps every other OEM punctuation accelerator two-part", () => {
    // Only '+' collides with the delimiter; the rest stay literal.
    for (const vk of [0xba, 0xbc, 0xbd, 0xbe, 0xbf, 0xc0, 0xdb, 0xdc, 0xdd, 0xde]) {
      const accel = buildAccelerator({ vk, ctrl: true });
      expect(accel.split("+").filter(Boolean), `vk=${vk.toString(16)}`).toHaveLength(2);
    }
  });

  it("returns an empty accelerator for keys with no usable name", () => {
    // Esc exits capture mode engine-side; any unmapped vk must not write a
    // modifier-only hotkey like "Ctrl".
    expect(buildAccelerator({ vk: 0x1b, ctrl: true })).toBe("");
    expect(buildAccelerator({ vk: 0xbb })).toBe("Plus");
    expect(buildAccelerator({ vk: 0x41, ctrl: true, shift: true })).toBe("Ctrl+Shift+A");
  });
});
