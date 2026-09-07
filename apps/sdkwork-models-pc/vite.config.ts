import { resolveViteEnvironment, resolveLucideReactEntry } from '../../../sdkwork-specs/tools/vite-runtime-profile.mjs';
import { resolveBrowserDistOutDir } from '../../../sdkwork-specs/tools/browser-dist-layout.mjs';



import { cpSync, createReadStream, existsSync, statSync } from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import type { Plugin } from "vite";
import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

const appRoot = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = join(appRoot, "../..");
const catalogMountPath = "/__sdkwork_catalog";

function createCatalogMiddleware(root: string) {
  const rootResolved = resolve(root);
  return (req: { url?: string }, res: { setHeader: (name: string, value: string) => void; statusCode: number; end: (body?: string) => void }, next: () => void) => {
    const requestUrl = req.url ?? "";
    if (!requestUrl.startsWith(catalogMountPath)) {
      next();
      return;
    }
    const relativePath = requestUrl.slice(catalogMountPath.length).replace(/^\//, "");
    if (relativePath.includes("..")) {
      res.statusCode = 400;
      res.end("invalid catalog path");
      return;
    }
    const filePath = resolve(rootResolved, relativePath);
    if (filePath !== rootResolved && !filePath.startsWith(`${rootResolved}${sep}`)) {
      res.statusCode = 403;
      res.end("forbidden");
      return;
    }
    if (!existsSync(filePath) || !statSync(filePath).isFile()) {
      res.statusCode = 404;
      res.end("catalog file not found");
      return;
    }
    res.setHeader("Content-Type", "application/json; charset=utf-8");
    createReadStream(filePath).pipe(res as unknown as NodeJS.WritableStream);
  };
}

function catalogAssetsPlugin(root: string): Plugin {
  return {
    name: "sdkwork-models-catalog-assets",
    configureServer(server) {
      server.middlewares.use(createCatalogMiddleware(root));
    },
    configurePreviewServer(server) {
      server.middlewares.use(createCatalogMiddleware(root));
    },
    closeBundle() {
      const outputRoot = join(appRoot, "dist", catalogMountPath.slice(1));
      for (const relativePath of ["sdkwork-models.json", "models", "sources", "schemas"]) {
        const sourcePath = join(root, relativePath);
        if (!existsSync(sourcePath)) {
          continue;
        }
        cpSync(sourcePath, join(outputRoot, relativePath), { recursive: true });
      }
    },
  };
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, appRoot, "");

  return {
  plugins: [react(), catalogAssetsPlugin(repositoryRoot)],
  root: appRoot,
  define: {
    "process.env.SDKWORK_MODELS_CATALOG_ROOT": JSON.stringify(catalogMountPath),
    "process.env.SDKWORK_ACCESS_TOKEN": JSON.stringify(env.SDKWORK_ACCESS_TOKEN ?? ""),
  },
  resolve: {
    alias: {
    },
  },
  build: {
    outDir: resolveBrowserDistOutDir(resolveViteEnvironment(mode, env)),
    emptyOutDir: true,
  },
};
});
