#!/usr/bin/env bun
/**
 * versioning — the semver automation CLI (U1+).
 *
 * Commands self-register: drop `commands/<name>.ts` exporting
 * `<camelName>Command(root, args)` plus an optional `usage` string.
 * Adding a command never requires editing this file.
 */
import { readdirSync } from "node:fs";
import { join } from "node:path";

type Command = (root: string, args: string[]) => number;

const camel = (s: string): string =>
  s.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());

async function loadCommands(): Promise<
  Map<string, { run: Command; usage: string }>
> {
  const dir = join(import.meta.dir, "commands");
  const commands = new Map<string, { run: Command; usage: string }>();
  for (const file of readdirSync(dir).sort()) {
    if (!file.endsWith(".ts") || file.endsWith(".test.ts")) continue;
    const name = file.slice(0, -3);
    const mod = (await import(join(dir, file))) as Record<string, unknown>;
    const run = mod[`${camel(name)}Command`];
    if (typeof run !== "function") {
      throw new Error(
        `commands/${file}: expected export ${camel(name)}Command`,
      );
    }
    commands.set(name, {
      run: run as Command,
      usage: typeof mod.usage === "string" ? mod.usage : name,
    });
  }
  return commands;
}

async function main(argv: string[]): Promise<number> {
  const [cmd, ...rest] = argv;
  const commands = await loadCommands();

  // --root is a global flag for tests / non-CWD invocations.
  let root = process.cwd();
  const args: string[] = [];
  for (let i = 0; i < rest.length; i++) {
    if (rest[i] === "--root") {
      root = rest[++i] ?? root;
    } else {
      args.push(rest[i]);
    }
  }

  if (cmd === undefined || cmd === "--help" || cmd === "-h") {
    const lines = [...commands.values()]
      .map((c) => `  ${c.usage}`)
      .join("\n");
    process.stdout.write(
      `usage: versioning <command> [args]\n\ncommands:\n${lines}\n`,
    );
    return cmd === undefined ? 2 : 0;
  }

  const entry = commands.get(cmd);
  if (!entry) {
    process.stderr.write(`unknown command "${cmd}"\n`);
    return 2;
  }
  return entry.run(root, args);
}

process.exit(await main(process.argv.slice(2)));
