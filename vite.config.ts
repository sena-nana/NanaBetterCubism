/// <reference types="vitest" />
import { defineLiliaViteConfig } from "@lilia/config";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));

export default defineLiliaViteConfig({
  test: {
    maxWorkers: 2,
  },
  vite: {
    resolve: {
      dedupe: ["vue", "vue-router", "@lucide/vue"],
      alias: {
        "@lucide/vue": `${root}/node_modules/@lucide/vue`,
      },
    },
  },
});
