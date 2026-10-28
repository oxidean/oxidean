import type { RepoPublic, RepoVisibility } from "@oxidean/api-client";

import { useState } from "octane";
import { TopicChipsInput } from "@/components/repo/topics-input";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { useRepoStore } from "@/lib/repo-store";
import { toastError, toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const PRIVATE_HELPER = "Only people with access can see this private repository.";

/** Repo Settings → General — description, homepage, topics, and visibility. */
export function GeneralSettingsPanel(props: {
  owner: string;
  name: string;
  repo: RepoPublic;
}) @{
  const owner = props.owner;
  const repoName = props.name;
  const repo = props.repo;
  const setRepo = useRepoStore((s) => s.setRepo);

  const [visibility, setVisibility] = useState<RepoVisibility>(() => repo.visibility);
  const [visPending, setVisPending] = useState(false);
  const [description, setDescription] = useState(() => repo.description ?? "");
  const [homepage, setHomepage] = useState(() => repo.homepage ?? "");
  const [editTopics, setEditTopics] = useState<string[]>(() => repo.topics ?? []);
  const [metaPending, setMetaPending] = useState(false);

  async function saveVisibility() {
    setVisPending(true);
    try {
      const res = await apiClient.repo.updateVisibility({
        owner,
        name: repoName,
        visibility,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not update visibility.");
        setVisPending(false);
        return;
      }
      setVisibility(res.data.visibility);
      setRepo(res.data);
      toastSuccess("Visibility updated.");
      setVisPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setVisPending(false);
    }
  }

  async function saveMetadata() {
    setMetaPending(true);
    try {
      const res = await apiClient.repo.updateMetadata({
        owner,
        name: repoName,
        description,
        homepage: homepage.trim(),
        topics: editTopics,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not update repository details.");
        setMetaPending(false);
        return;
      }
      setDescription(res.data.description ?? "");
      setHomepage(res.data.homepage ?? "");
      setEditTopics(res.data.topics ?? []);
      setRepo(res.data);
      toastSuccess("Repository details updated.");
      setMetaPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setMetaPending(false);
    }
  }

  <>
    <section className="flex flex-col gap-4" aria-labelledby="general-heading">
      <h2
        id="general-heading"
        className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
      >
        General
      </h2>
      <p className="text-[14px] text-muted-foreground">
        Description, homepage, and topics shown on the Code home About panel.
      </p>
      <div className="flex flex-col gap-2">
        <Label htmlFor="repo-description">Description</Label>
        <Textarea
          id="repo-description"
          value={description}
          onInput={(e) => setDescription((e.currentTarget as HTMLTextAreaElement).value)}
          disabled={metaPending}
          rows={3}
          maxLength={350}
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label htmlFor="repo-homepage">Homepage</Label>
        <Input
          id="repo-homepage"
          value={homepage}
          onInput={(e) => setHomepage((e.currentTarget as HTMLInputElement).value)}
          disabled={metaPending}
          placeholder="https://example.com"
          autoComplete="off"
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label htmlFor="repo-topics">Topics</Label>
        <TopicChipsInput
          id="repo-topics"
          topics={editTopics}
          onTopicsChange={setEditTopics}
          disabled={metaPending}
          placeholder="Add topics…"
        />
        <p className="text-[13px] text-muted-foreground">
          Lowercase slugs (letters, digits, hyphens) — comma or Enter adds a topic.
        </p>
      </div>
      <Button
        type="button"
        disabled={metaPending}
        onClick={() => void saveMetadata()}
        className="w-full sm:w-auto"
      >
        {(metaPending ? "Saving…" : "Save details") as string}
      </Button>
    </section>

    <section className="mt-12 flex flex-col gap-4" aria-labelledby="vis-heading">
      <h2
        id="vis-heading"
        className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
      >
        Visibility
      </h2>
      <p className="text-[14px] text-muted-foreground">Choose who can see this repository.</p>
      <div className="flex flex-wrap gap-2" role="group" aria-label="Visibility">
        <Button
          type="button"
          variant={visibility === "public" ? "default" : "secondary"}
          aria-pressed={visibility === "public"}
          disabled={visPending}
          onClick={() => setVisibility("public")}
        >
          Public
        </Button>
        <Button
          type="button"
          variant={visibility === "private" ? "default" : "secondary"}
          aria-pressed={visibility === "private"}
          disabled={visPending}
          onClick={() => setVisibility("private")}
        >
          Private
        </Button>
      </div>
      @if (visibility === "private") {
        <p className="text-[14px] text-muted-foreground">{PRIVATE_HELPER as string}</p>;
      }
      <Button
        type="button"
        disabled={visPending || visibility === repo.visibility}
        onClick={() => void saveVisibility()}
        className="w-full sm:w-auto"
      >
        {(visPending ? "Saving…" : "Save visibility") as string}
      </Button>
    </section>
  </>;
}
