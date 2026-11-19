import { describe, expect, it } from "vitest";
import { errorText, MAX_ERROR_TEXT } from "$lib/errors";

describe("errorText", () => {
  it("Error 取 message", () => {
    expect(errorText(new Error("boom"))).toBe("boom");
  });

  it("空 message 回退到 name", () => {
    expect(errorText(new TypeError())).toBe("TypeError");
  });

  it("字符串与空值兜底", () => {
    expect(errorText("  直接字符串  ")).toBe("直接字符串");
    expect(errorText(null)).toBe("未知错误");
    expect(errorText(undefined)).toBe("未知错误");
    expect(errorText("  ")).toBe("未知错误");
  });

  it("超长文本按上限截断并加省略号", () => {
    expect(errorText("x".repeat(300))).toBe("x".repeat(MAX_ERROR_TEXT) + "…");
    expect(errorText("abcdef", 3)).toBe("abc…");
    expect(errorText("abc", 3)).toBe("abc");
  });
});
