import type { RepoTemplateOption } from "@oxidean/api-client";
import { useState } from "octane";
import { TemplatePicker } from "@/components/repo/template-picker";

const HARNESS_STACKS: RepoTemplateOption[] = [
  {
    id: "rust",
    label: "Rust",
    group: "Systems",
    description: "Cargo binary crate.",
    default_gitignore: "Rust",
    provenance: "builtin",
  },
  {
    id: "go",
    label: "Go",
    group: "Backend",
    description: "Go module.",
    default_gitignore: "Go",
    provenance: "builtin",
  },
  {
    id: "nextjs",
    label: "Next.js",
    group: "Frontend",
    description: "Next app router.",
    default_gitignore: "Node",
    provenance: "builtin",
  },
  {
    id: "instance:p1",
    label: "Acme Node",
    group: "Custom",
    description: "Instance pack.",
    default_gitignore: "Node",
    provenance: "instance",
    source_label: "acme-node",
  },
  {
    id: "repo:r1",
    label: "starter",
    group: "From template",
    description: "User template repo.",
    provenance: "user",
    source_label: "ada/starter",
  },
];

const GITIGNORES: RepoTemplateOption[] = [
  {
    id: "Rust",
    label: "Rust",
    group: "Languages",
    description: "target/",
  },
  {
    id: "Go",
    label: "Go",
    group: "Languages",
    description: "bin/",
  },
  {
    id: "Node",
    label: "Node",
    group: "Languages",
    description: "node_modules/",
  },
];

/** Mirrors /new: stack pick autofills .gitignore while the modal closes. */
export function TemplatePickerSiblingHarness() @{
  const [stackId, setStackId] = useState("none");
  const [gitignoreId, setGitignoreId] = useState("none");

  <div className="flex flex-col gap-5 p-4">
    <TemplatePicker
      id="repo-stack"
      label="Stack / template"
      description="Pick a starter."
      value={stackId}
      options={HARNESS_STACKS}
      noneLabel="None"
      noneDescription="Empty."
      onChange={(id) => {
        setStackId(id);
        if (id === "none") {
          setGitignoreId("none");
          return;
        }
        const pack = HARNESS_STACKS.find((s) => s.id === id);
        setGitignoreId(pack?.default_gitignore ?? "none");
      }}
    />
    <TemplatePicker
      id="repo-gitignore"
      label=".gitignore"
      description="Auto-filled from the stack."
      value={gitignoreId}
      options={GITIGNORES}
      noneLabel="None"
      noneDescription="Skip."
      onChange={setGitignoreId}
    />
    <p data-testid="stack-value">{stackId as string}</p>
    <p data-testid="gitignore-value">{gitignoreId as string}</p>
  </div>;
}
