import { describe, expect, it } from "vitest";
import { isDismissShortcut, isSettingsShortcut, settingsHint, type KeyLike } from "$lib/shortcuts";

const key = (k: string, mods: Partial<KeyLike> = {}): KeyLike => ({
  key: k, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods,
});

describe("isSettingsShortcut", () => {
  it("Ctrl+, 与 Cmd+, 生效", () => {
    expect(isSettingsShortcut(key(",", { ctrlKey: true }))).toBe(true);
    expect(isSettingsShortcut(key(",", { metaKey: true }))).toBe(true);
  });

  it("带 Alt/Shift 或无修饰键不生效", () => {
    expect(isSettingsShortcut(key(","))).toBe(false);
    expect(isSettingsShortcut(key(",", { ctrlKey: true, altKey: true }))).toBe(false);
    expect(isSettingsShortcut(key(",", { ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(isSettingsShortcut(key("s", { ctrlKey: true }))).toBe(false);
  });
});

describe("isDismissShortcut / settingsHint", () => {
  it("Esc 关闭浮层", () => {
    expect(isDismissShortcut(key("Escape"))).toBe(true);
    expect(isDismissShortcut(key("Enter"))).toBe(false);
  });

  it("提示随平台变化", () => {
    expect(settingsHint(true)).toBe("设置(Cmd+,)");
    expect(settingsHint(false)).toBe("设置(Ctrl+,)");
  });
});
