/**
 * Chromium gate for the workflow-template catalog — client-side search must
 * re-render card sections through the real DOM without Octane overlay/races.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { act } from "octane";
import { WorkflowTemplateCatalog } from "@/components/repo/workflow-template-catalog";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const TEMPLATES = {
  suggested: [
    {
      id: "builtin:ci/rust",
      name: "Rust",
      description: "Build and test a Rust project with Cargo.",
      icon_name: "rust",
      icon_url: "/api/actions/template-icons/rust.svg",
      categories: ["Rust", "Continuous integration"],
      file_patterns: ["Cargo.toml"],
      filename: "rust.yml",
      provenance: "builtin" as const,
      source_label: null,
      suggested: true,
      unsupported_actions: [],
      parse_error: null,
    },
  ],
  templates: [
    {
      id: "builtin:ci/rust",
      name: "Rust",
      description: "Build and test a Rust project with Cargo.",
      icon_name: "rust",
      icon_url: "/api/actions/template-icons/rust.svg",
      categories: ["Rust", "Continuous integration"],
      file_patterns: ["Cargo.toml"],
      filename: "rust.yml",
      provenance: "builtin" as const,
      source_label: null,
      suggested: true,
      unsupported_actions: [],
      parse_error: null,
    },
    {
      id: "builtin:deployments/aws",
      name: "Deploy to AWS",
      description: "Ship the app to AWS.",
      icon_name: null,
      icon_url: null,
      categories: ["Deployment"],
      file_patterns: [],
      filename: "aws.yml",
      provenance: "builtin" as const,
      source_label: null,
      suggested: false,
      unsupported_actions: ["aws-actions/configure-aws-credentials"],
      parse_error: null,
    },
  ],
  categories: ["Deployment", "Continuous integration"],
  total_count: 2,
};

const LONG_ACTION_TEMPLATES = {
  suggested: [],
  templates: [
    {
      id: "builtin:deployments/octopus",
      name: "Build and Deploy with Octopus Deploy",
      description: "Build, release, and deploy with Octopus Deploy.",
      icon_name: null,
      icon_url: null,
      categories: ["Deployment"],
      file_patterns: [],
      filename: "octopus.yml",
      provenance: "builtin" as const,
      source_label: null,
      suggested: false,
      unsupported_actions: [
        "OctopusDeploy/create-release-action@fea7e7b45c38c021b6bc5a14bd7eaa2ed5269214",
        "OctopusDeploy/deploy-release-action@b10a606c903b0a5bce24102af9d066638ab429ac",
        "OctopusDeploy/login-action@34b6dcc1e86fa373c14e6a28c5507d221e4de622b8582d",
        "docker/login-action@9780b0c442fbb1117ed29e0efdff1e18412f7567",
        "OctopusDeploy/push-package-action@aea43c0359b39d1a7b898a7553e649f3f40b88f9",
      ],
      parse_error: null,
    },
  ],
  categories: ["Deployment"],
  total_count: 1,
};

async function waitForTestId(testId: string, ms = 5_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("WorkflowTemplateCatalog browser DOM races", () => {
  it("search input filters cards through the real DOM", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(WorkflowTemplateCatalog, {
        owner: "ada",
        repo: "hello",
        data: TEMPLATES,
        loading: false,
        error: null,
      });
      await waitForTestId("workflow-template-card-builtin:deployments/aws");

      const search = (await waitForTestId("workflow-template-search")) as HTMLInputElement;
      await act(async () => {
        search.focus();
        search.value = "aws";
        search.dispatchEvent(new Event("input", { bubbles: true }));
      });

      await vi.waitFor(() => {
        expect(
          document.querySelector('[data-testid="workflow-template-card-builtin:ci/rust"]'),
        ).toBeNull();
        expect(
          document.querySelector('[data-testid="workflow-template-card-builtin:deployments/aws"]'),
        ).toBeTruthy();
      });
      expectNoOctaneOverlayInDocument();
      expect(tracker.errors).toEqual([]);
    } finally {
      tracker.dispose();
    }
  });

  it("long action@sha notes stay inside the card — no horizontal overflow", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(WorkflowTemplateCatalog, {
        owner: "ada",
        repo: "hello",
        data: LONG_ACTION_TEMPLATES,
        loading: false,
        error: null,
      });
      const card = (await waitForTestId(
        "workflow-template-card-builtin:deployments/octopus",
      )) as HTMLElement;

      const note = card.querySelector("p[title]") as HTMLElement;
      expect(note).toBeTruthy();
      expect(note.textContent).toContain("OctopusDeploy/create-release-action");
      expect(note.textContent).toContain("+2 more");
      expect(note.textContent).not.toContain("@fea7e7b");

      expect(note.scrollWidth).toBeLessThanOrEqual(note.clientWidth + 1);
      expect(card.scrollWidth).toBeLessThanOrEqual(card.clientWidth + 1);
      expect(
        (document.querySelector('[data-testid="workflow-template-catalog"]') as HTMLElement)
          .scrollWidth,
      ).toBeLessThanOrEqual(document.documentElement.clientWidth + 1);
      expectNoOctaneOverlayInDocument();
      expect(tracker.errors).toEqual([]);
    } finally {
      tracker.dispose();
    }
  });
});
