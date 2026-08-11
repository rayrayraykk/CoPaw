import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

export default defineConfig(({ mode }) => {
  const rootDir = fileURLToPath(new URL(".", import.meta.url));
  const env = loadEnv(mode, rootDir, "");

  return {
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: {
        "@": fileURLToPath(new URL("./src", import.meta.url)),
      },
    },
    base: env.VITE_BASE_PATH || "/",
    build: {
      rollupOptions: {
        input: {
          main: fileURLToPath(new URL("./index.html", import.meta.url)),
          "llm-wiki-graph": fileURLToPath(
            new URL(
              "./prototypes/llm-wiki-graph/index.html",
              import.meta.url,
            ),
          ),
          "llm-wiki-nebula": fileURLToPath(
            new URL(
              "./prototypes/llm-wiki-graph/nebula.html",
              import.meta.url,
            ),
          ),
          "llm-wiki-galaxy": fileURLToPath(
            new URL(
              "./prototypes/llm-wiki-graph/galaxy.html",
              import.meta.url,
            ),
          ),
          "llm-wiki-inference": fileURLToPath(
            new URL(
              "./prototypes/llm-wiki-graph/inference.html",
              import.meta.url,
            ),
          ),
          "llm-wiki-temporal": fileURLToPath(
            new URL(
              "./prototypes/llm-wiki-graph/temporal.html",
              import.meta.url,
            ),
          ),
          "llm-wiki-focus": fileURLToPath(
            new URL(
              "./prototypes/llm-wiki-graph/focus.html",
              import.meta.url,
            ),
          ),
        },
        output: {
          manualChunks: {
            markdown: [
              "react-markdown",
              "remark-gfm",
              "rehype-highlight",
              "rehype-raw",
              "highlight.js",
              "react-syntax-highlighter",
            ],
            mermaid: ["mermaid"],
            router: ["react-router-dom"],
            i18n: ["i18next", "react-i18next"],
          },
        },
      },
    },
    optimizeDeps: {
      include: [
        "react",
        "react-dom",
        "react-router-dom",
        "i18next",
        "react-i18next",
      ],
    },
  };
});
