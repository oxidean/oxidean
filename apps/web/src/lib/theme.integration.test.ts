import { afterEach, describe, expect, it, vi } from "vitest";
import {
  THEME_BOOT_SCRIPT,
  THEME_COOKIE_KEY,
  THEME_OPTIONS,
  THEME_RESOLVED_COOKIE_KEY,
  THEME_STORAGE_KEY,
  applyTheme,
  readThemePreference,
  resolveTheme,
} from "./theme";

describe("theme helpers", () => {
  afterEach(() => {
    localStorage.removeItem(THEME_STORAGE_KEY);
    document.documentElement.classList.remove("dark");
    document.cookie = `${THEME_COOKIE_KEY}=; path=/; max-age=0`;
    document.cookie = `${THEME_RESOLVED_COOKIE_KEY}=; path=/; max-age=0`;
    vi.restoreAllMocks();
  });

  it("defaults preference to system", () => {
    expect(readThemePreference()).toBe("system");
  });

  it("treats invalid stored preference as system (BRAND-04)", () => {
    localStorage.setItem(THEME_STORAGE_KEY, "purple");
    expect(readThemePreference()).toBe("system");
  });

  it("resolveTheme honors explicit light/dark", () => {
    expect(resolveTheme("light")).toBe("light");
    expect(resolveTheme("dark")).toBe("dark");
  });

  it("resolveTheme follows OS prefers-color-scheme when preference is system (BRAND-04)", () => {
    const matchMedia = vi.spyOn(window, "matchMedia").mockImplementation(
      (query: string) =>
        ({
          matches: query.includes("prefers-color-scheme: dark"),
          media: query,
          onchange: null,
          addListener: () => undefined,
          removeListener: () => undefined,
          addEventListener: () => undefined,
          removeEventListener: () => undefined,
          dispatchEvent: () => false,
        }) as MediaQueryList,
    );

    expect(resolveTheme("system")).toBe("dark");
    matchMedia.mockImplementation(
      (query: string) =>
        ({
          matches: false,
          media: query,
          onchange: null,
          addListener: () => undefined,
          removeListener: () => undefined,
          addEventListener: () => undefined,
          removeEventListener: () => undefined,
          dispatchEvent: () => false,
        }) as MediaQueryList,
    );
    expect(resolveTheme("system")).toBe("light");
  });

  it("applyTheme persists preference across read (BRAND-05)", () => {
    applyTheme("dark");
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
    expect(readThemePreference()).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.cookie).toMatch(/oxidean-theme=dark/);
    expect(document.cookie).toMatch(/oxidean-color-scheme=dark/);

    applyTheme("light");
    expect(readThemePreference()).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(document.cookie).toMatch(/oxidean-theme=light/);
    expect(document.cookie).toMatch(/oxidean-color-scheme=light/);

    applyTheme("system");
    expect(readThemePreference()).toBe("system");
    expect(THEME_OPTIONS).toEqual(["system", "light", "dark"]);
  });

  it("THEME_BOOT_SCRIPT is a static FOUC boot that reads oxidean-theme (D-12)", () => {
    expect(THEME_BOOT_SCRIPT).toContain('localStorage.getItem("oxidean-theme")');
    expect(THEME_BOOT_SCRIPT).toContain("prefers-color-scheme: dark");
    expect(THEME_BOOT_SCRIPT).toContain('classList.toggle("dark"');
    expect(THEME_BOOT_SCRIPT).toContain("oxidean-color-scheme=");
    expect(THEME_BOOT_SCRIPT).not.toMatch(/\$\{|`/);
  });
});
