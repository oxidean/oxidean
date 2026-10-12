import { repoTopicsSuggestQueryOptions } from "@oxidean/api-client";
import { useQuery } from "@octanejs/tanstack-query";
import { X } from "@octanejs/lucide";
import { useEffect, useState } from "octane";
import { apiClient } from "@/lib/api-client";
import { cn } from "@/lib/utils";

const MAX_REPO_TOPICS = 20;

type TopicChipsInputProps = {
  id?: string;
  /** Controlled chip list (normalized topic slugs). */
  topics: string[];
  onTopicsChange: (next: string[]) => void;
  disabled?: boolean;
  placeholder?: string;
  /** Accessible name when no visible <Label> is associated. */
  ariaLabel?: string;
  /** Max chips allowed (API enforces 20). */
  max?: number;
};

/**
 * Normalize freeform text to a topic slug: lowercase, runs of
 * non-`[a-z0-9-]` collapse to `-`, edges trimmed, ≤ 50 chars.
 * Mirrors `normalize_topic_name` in oxidean-db.
 */
export function normalizeTopicSlug(raw: string): string {
  return raw
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/-{2,}/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 50);
}

/**
 * Topics editor: committed topics render as chips inside the
 * input box; freeform text commits on `,` / Enter / autocomplete pick;
 * Backspace on empty text removes the last chip. Suggestions come from
 * `repo.topicsSuggest` (most-used first) while the field is focused.
 */
export function TopicChipsInput({
  id = "topics-input",
  topics,
  onTopicsChange,
  disabled,
  placeholder,
  ariaLabel,
  max = MAX_REPO_TOPICS,
}: TopicChipsInputProps) @{
  const [text, setText] = useState("");
  const [debounced, setDebounced] = useState("");
  const [focused, setFocused] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);

  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(text.trim()), 200);
    return () => window.clearTimeout(t);
  }, [text]);

  const atCap = topics.length >= max;
  const suggQ = useQuery({
    ...repoTopicsSuggestQueryOptions(apiClient, { q: debounced, limit: 8 }),
    enabled: focused && !atCap && !disabled,
  });
  const suggestions = (suggQ.data?.topics ?? [])
    .filter((t) => !topics.includes(t.name))
    .slice(0, 8);
  const open = focused && !dismissed && !atCap && suggestions.length > 0;

  function commitInto(list: string[], raw: string): string[] {
    const name = normalizeTopicSlug(raw);
    if (name && !list.includes(name) && list.length < max) {
      return [...list, name];
    }
    return list;
  }

  function commitRaw(raw: string) {
    onTopicsChange(commitInto(topics, raw));
  }

  function commitText() {
    commitRaw(text);
    setText("");
    setActiveIndex(-1);
  }

  function removeTopic(name: string) {
    onTopicsChange(topics.filter((t) => t !== name));
  }

  function focusInput() {
    document.getElementById(id)?.focus();
  }

  function onInput(ev: Event) {
    const v = (ev.currentTarget as HTMLInputElement).value;
    setDismissed(false);
    if (!v.includes(",")) {
      setText(v);
      return;
    }
    // Paste / type `a,b,c` — commit complete segments, keep the tail.
    // Accumulate locally: `topics` prop is stale within this handler.
    const parts = v.split(",");
    const tail = parts.pop() ?? "";
    let next = topics;
    for (const p of parts) next = commitInto(next, p);
    onTopicsChange(next);
    setText(tail);
  }

  function onKeyDown(ev: KeyboardEvent) {
    if (ev.key === "," || ev.key === "Enter") {
      ev.preventDefault();
      if (ev.key === "Enter" && open && activeIndex >= 0) {
        const hit = suggestions[activeIndex];
        if (hit) {
          commitRaw(hit.name);
          setText("");
          setActiveIndex(-1);
          return;
        }
      }
      commitText();
      return;
    }
    if (ev.key === "Tab" && open && activeIndex >= 0) {
      const hit = suggestions[activeIndex];
      if (hit) {
        ev.preventDefault();
        commitRaw(hit.name);
        setText("");
        setActiveIndex(-1);
      }
      return;
    }
    if (ev.key === "Backspace" && text === "" && topics.length > 0) {
      ev.preventDefault();
      onTopicsChange(topics.slice(0, -1));
      return;
    }
    if (!open || suggestions.length === 0) return;
    if (ev.key === "ArrowDown") {
      ev.preventDefault();
      setActiveIndex((i) => (i + 1) % suggestions.length);
    } else if (ev.key === "ArrowUp") {
      ev.preventDefault();
      setActiveIndex((i) => (i <= 0 ? suggestions.length - 1 : i - 1));
    } else if (ev.key === "Escape") {
      setDismissed(true);
      setActiveIndex(-1);
    }
  }

  <div className="relative" data-testid={`${id}-chips`}>
    <div
      className={cn(
        "flex min-h-9 w-full flex-wrap items-center gap-1.5 rounded-md border border-input bg-background px-2 py-1 text-[14px] transition-[border-color,box-shadow] duration-150",
        "focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/40",
        disabled && "cursor-not-allowed border-border bg-muted/40 opacity-70",
      )}
      onClick={() => {
        if (!disabled) focusInput();
      }}
    >
      @for (const topic of topics; key topic) {
        <span className="inline-flex items-center gap-1 rounded-full border border-border bg-primary/10 py-0.5 pl-2.5 pr-1 text-[12px] text-primary">
          {topic as string}
          <button
            type="button"
            className="inline-flex h-4 w-4 items-center justify-center rounded-full text-primary/70 hover:bg-primary/15 hover:text-primary"
            aria-label={`Remove topic ${topic}`}
            disabled={disabled}
            onClick={(ev) => {
              ev.stopPropagation();
              removeTopic(topic);
            }}
          >
            <X aria-hidden size={11} />
          </button>
        </span>;
      }
      @if (!atCap) {
        <input
          id={id}
          type="text"
          role="combobox"
          aria-label={(ariaLabel ?? "Topics") as string}
          aria-expanded={open}
          aria-controls={`${id}-listbox`}
          aria-autocomplete="list"
          aria-activedescendant={
            (activeIndex >= 0 ? `${id}-opt-${activeIndex}` : undefined) as string | undefined
          }
          className="min-w-[8ch] flex-1 bg-transparent py-0.5 text-[14px] text-foreground outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:text-muted-foreground"
          value={text}
          placeholder={(topics.length === 0 ? (placeholder ?? "Add topics…") : "") as string}
          autoComplete="off"
          disabled={disabled}
          onInput={onInput}
          onKeyDown={onKeyDown}
          onFocus={() => {
            setFocused(true);
            setDismissed(false);
          }}
          onBlur={() => {
            window.setTimeout(() => setFocused(false), 150);
          }}
        />;
      }
    </div>
    @if (open) {
      <ul
        id={`${id}-listbox`}
        role="listbox"
        aria-label="Topic suggestions"
        className="absolute top-full z-20 mt-1 max-h-56 w-full overflow-auto rounded-md border border-border bg-card py-1 shadow-md"
        data-testid={`${id}-listbox`}
      >
        @for (const hit of suggestions; key hit.name) {
          <li
            role="option"
            id={`${id}-opt-${suggestions.indexOf(hit)}`}
            aria-selected={suggestions[activeIndex]?.name === hit.name}
          >
            <button
              type="button"
              className={cn(
                "flex w-full items-center gap-3 px-3 py-1.5 text-left text-[14px] outline-none",
                "hover:bg-muted focus-visible:bg-muted",
                suggestions[activeIndex]?.name === hit.name && "bg-muted",
              )}
              onMouseDown={(ev) => {
                ev.preventDefault();
                commitRaw(hit.name);
                setText("");
                setActiveIndex(-1);
                focusInput();
              }}
            >
              <span className="min-w-0 flex-1 truncate font-medium text-foreground">
                {hit.name as string}
              </span>
              <span className="shrink-0 text-[12px] text-muted-foreground">
                {
                  `${hit.repo_count} ${hit.repo_count === 1 ? "repo" : "repos"}` as string
                }
              </span>
            </button>
          </li>;
        }
      </ul>;
    }
  </div>;
}
