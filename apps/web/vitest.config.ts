import path from "node:path";
import { fileURLToPath } from "node:url";
import { octane } from "@octanejs/vite-plugin";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";
import {
  fixTypeOnlyImports,
  RECHARTS_TYPE_ONLY_IMPORT_FIX,
} from "./vite-plugins/fix-type-only-imports.ts";
import {
  ensureAuthSettings,
  expectActionsPipelineFlow,
  expectAdminLfsQuotasFlow,
  expectAuthMeDedupedOnHome,
  expectBranchDialogsFlow,
  expectChromeCreateAndAccountMenusFlow,
  expectForgeIssuesCrudFlow,
  expectForgeReleasesCrudFlow,
  expectForgeRepoPackagesFlow,
  expectForgeSshAndOrgMembersFlow,
  expectMirrorAuthToggleFlow,
  expectMobileNavTapFlow,
  expectNewRepoTemplatePickerFlow,
  expectPackagesVisualFlow,
  expectPatMintClickThroughFlow,
  expectReleaseAssetFlow,
  expectRepoRenameTransferFlow,
  expectSettingsProfileAvatarFlow,
  expectStatusHealthy,
  expectWorkosCta,
  loginThroughOidc,
  restoreLocalAuthCommand,
  signupThroughUi,
} from "./e2e/stack-browser/commands.ts";

const rootDir = path.dirname(fileURLToPath(import.meta.url));
const stackEnabled = process.env.E2E_STACK === "1";

/** Static `process.env.KEY` replacements so browser tests see harness env. */
const stackEnvDefine = {
  "process.env.E2E_STACK": JSON.stringify(process.env.E2E_STACK ?? ""),
  "process.env.OXIDEAN_E2E_API_ORIGIN": JSON.stringify(
    process.env.OXIDEAN_E2E_API_ORIGIN ?? "http://127.0.0.1:18080",
  ),
  "process.env.OXIDEAN_E2E_WEB_ORIGIN": JSON.stringify(
    process.env.OXIDEAN_E2E_WEB_ORIGIN ?? "http://127.0.0.1:13000",
  ),
  "process.env.OXIDEAN_E2E_MAILPIT_ORIGIN": JSON.stringify(
    process.env.OXIDEAN_E2E_MAILPIT_ORIGIN ?? "http://127.0.0.1:8025",
  ),
  "process.env.OXIDEAN_E2E_STUBS_ORIGIN": JSON.stringify(
    process.env.OXIDEAN_E2E_STUBS_ORIGIN ?? "http://127.0.0.1:9092",
  ),
  "process.env.OXIDEAN_E2E_ADMIN_EMAIL": JSON.stringify(
    process.env.OXIDEAN_E2E_ADMIN_EMAIL ?? "admin@oxidean.local",
  ),
  "process.env.OXIDEAN_E2E_ADMIN_PASSWORD": JSON.stringify(
    process.env.OXIDEAN_E2E_ADMIN_PASSWORD ?? "password1",
  ),
  "process.env.OXIDEAN_E2E_OIDC_ISSUER": JSON.stringify(
    process.env.OXIDEAN_E2E_OIDC_ISSUER ?? "http://127.0.0.1:9090/default",
  ),
  "process.env.OXIDEAN_E2E_DB_PATH": JSON.stringify(process.env.OXIDEAN_E2E_DB_PATH ?? ""),
};

/**
 * Vitest projects:
 * - unit / integration: default `bun run test` (explicit --project flags)
 * - browser: Chromium component tests (`bun run test:browser`) — real DOM for
 *   Octane insertBefore races that happy-dom misses (issues #41–#43)
 * - e2e-stack (+ e2e-stack-browser): only when E2E_STACK=1 (`make test-e2e-stack`)
 */
export default defineConfig({
  plugins: [fixTypeOnlyImports(RECHARTS_TYPE_ONLY_IMPORT_FIX), octane()],
  resolve: {
    alias: {
      "@": path.resolve(rootDir, "./src"),
      // ClientRouter isn't present in happy-dom — stub pushes real history.
      "astro:transitions/client": path.resolve(rootDir, "./src/test/astro-transitions-stub.ts"),
    },
  },
  // Pre-bundle deps Vite discovers mid-suite so Chromium iframes are
  // not aborted by an optimizeDeps reload (CI flake for the browser project).
  optimizeDeps: {
    include: [
      "class-variance-authority",
      "clsx",
      "tailwind-merge",
      // Markdown chain pulled by ReadmePanel/markdown components mid-suite.
      "rehype-sanitize",
      "rehype-stringify",
      "remark-gfm",
      "remark-github",
      "remark-parse",
      "remark-rehype",
      "unified",
    ],
  },
  test: {
    globals: false,
    coverage: {
      provider: "v8",
      reporter: ["text-summary", "json-summary", "lcov"],
      reportsDirectory: "./coverage",
      // Default: only files exercised by tests (keeps D-QH-02 floor meaningful).
      // Do not set `all: true` / broad include until suite depth catches up.
      exclude: [
        "**/*.{test,spec}.{ts,tsx}",
        "**/test/**",
        "e2e/**",
        "**/*.d.ts",
        "src/routeTree.gen.ts",
        "src/styles/**",
      ],
      reportOnFailure: true,
    },
    projects: [
      {
        extends: true,
        test: {
          name: "unit",
          environment: "node",
          include: [
            "src/**/*.unit.test.ts",
            "src/**/*.gate.test.ts",
            // Plan 07-14: markdown/highlight libs use *.test.ts (not *.unit.test.ts)
            "src/lib/markdown.test.ts",
            // Plan 11-01: ISS-04 issue autolink Wave 0 stubs
            "src/lib/markdown.issues.test.ts",
            "src/lib/highlight.test.ts",
          ],
        },
      },
      {
        extends: true,
        test: {
          name: "integration",
          environment: "happy-dom",
          include: ["src/**/*.integration.test.{ts,tsx}"],
          setupFiles: ["./src/test/setup-integration.ts"],
          // happy-dom + dynamic route imports contend under parallel workers (timeouts / DOM bleed).
          fileParallelism: false,
        },
      },
      {
        extends: true,
        test: {
          name: "browser",
          include: ["src/**/*.browser.test.{ts,tsx}"],
          setupFiles: ["./src/test/setup-browser.ts"],
          fileParallelism: false,
          testTimeout: 30_000,
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            instances: [{ browser: "chromium" as const }],
          },
        },
      },
      ...(stackEnabled
        ? [
            {
              extends: true as const,
              define: stackEnvDefine,
              test: {
                name: "e2e-stack",
                environment: "node" as const,
                include: ["e2e/stack/**/*.stack.test.ts"],
                fileParallelism: false,
                setupFiles: ["./e2e/stack/setup.ts"],
                testTimeout: 60_000,
              },
            },
            {
              extends: true as const,
              define: stackEnvDefine,
              test: {
                name: "e2e-stack-browser",
                include: ["e2e/stack-browser/**/*.stack.browser.test.{ts,tsx}"],
                setupFiles: ["./e2e/stack-browser/setup.ts"],
                fileParallelism: false,
                testTimeout: 60_000,
                browser: {
                  enabled: true,
                  provider: playwright(),
                  headless: true,
                  instances: [{ browser: "chromium" as const }],
                  commands: {
                    ensureAuthSettings,
                    restoreLocalAuthCommand,
                    signupThroughUi,
                    expectWorkosCta,
                    loginThroughOidc,
                    expectStatusHealthy,
                    expectAuthMeDedupedOnHome,
                    expectForgeRepoPackagesFlow,
                    expectForgeIssuesCrudFlow,
                    expectForgeReleasesCrudFlow,
                    expectForgeSshAndOrgMembersFlow,
                    expectAdminLfsQuotasFlow,
                    expectChromeCreateAndAccountMenusFlow,
                    expectNewRepoTemplatePickerFlow,
                    expectSettingsProfileAvatarFlow,
                    expectMirrorAuthToggleFlow,
                    expectMobileNavTapFlow,
                    expectPackagesVisualFlow,
                    expectBranchDialogsFlow,
                    expectActionsPipelineFlow,
                    expectPatMintClickThroughFlow,
                    expectReleaseAssetFlow,
                    expectRepoRenameTransferFlow,
                  },
                },
              },
            },
          ]
        : []),
    ],
  },
});
