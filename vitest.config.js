import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

// 纯逻辑 + rune 模块单测：$lib 指向 src/lib，svelte 插件编译 .svelte.ts 的 runes。
// 不加载 sveltekit()，避免拉起 Tauri/SSR 运行时。
const libDir = fileURLToPath(new URL("./src/lib", import.meta.url));

export default defineConfig({
  plugins: [svelte({ compilerOptions: { dev: true } })],
  resolve: {
    alias: { $lib: libDir },
    conditions: ["browser", "module"],
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    clearMocks: true,
    server: { deps: { inline: ["@sveltejs/vite-plugin-svelte"] } },
  },
});
