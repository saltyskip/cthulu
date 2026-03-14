export interface ThemeDefinition {
  id: string;
  label: string;
  group: "branded" | "preset";
  colorScheme: "dark" | "light";
  /** Bundled Shiki theme name, or a custom theme object */
  shikiTheme: string | Record<string, unknown>;
  vars: Record<string, string>;
}

import { eldritchDark, eldritchLight, toCssVarMap } from "@cthulu/brand";
import { eldritchShikiTheme } from "./shiki-eldritch";
import { eldritchLightShikiTheme } from "./shiki-eldritch-light";

const STORAGE_KEY = "cthulu_theme";

/**
 * Shared design tokens that every theme inherits.
 * Themes override color-specific vars; these provide typography, spacing, radius,
 * shadows, fonts, and transitions so the entire system stays consistent.
 */
const sharedTokens: Record<string, string> = {
  /* Typography scale */
  "text-2xs": "0.625rem",      /* 10px */
  "text-xs": "0.6875rem",      /* 11px */
  "text-sm": "0.8125rem",      /* 13px */
  "text-base": "0.875rem",     /* 14px */
  "text-lg": "1rem",           /* 16px */
  "text-xl": "1.25rem",        /* 20px */
  "text-2xl": "1.5rem",        /* 24px */

  /* Spacing scale (4px base) */
  "space-1": "0.25rem",        /* 4px */
  "space-2": "0.5rem",         /* 8px */
  "space-3": "0.75rem",        /* 12px */
  "space-4": "1rem",           /* 16px */
  "space-5": "1.25rem",        /* 20px */
  "space-6": "1.5rem",         /* 24px */
  "space-8": "2rem",           /* 32px */
  "space-10": "2.5rem",        /* 40px */

  /* Radius scale */
  "radius-sm": "4px",
  "radius-md": "8px",
  "radius-lg": "12px",
  "radius-xl": "16px",
  "radius-full": "9999px",

  /* Fonts */
  "font-sans": '"Inter Variable", "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  "font-mono": '"JetBrains Mono Variable", "JetBrains Mono", "SF Mono", "Fira Code", monospace',

  /* Transitions */
  "transition-fast": "100ms ease",
  "transition-base": "150ms ease",
  "transition-slow": "250ms ease-out",
};

/** Light-theme shadow tokens */
const lightShadows: Record<string, string> = {
  "shadow-sm": "0 1px 2px rgba(0, 0, 0, 0.05)",
  "shadow-md": "0 4px 6px -1px rgba(0, 0, 0, 0.07), 0 2px 4px -2px rgba(0, 0, 0, 0.05)",
  "shadow-lg": "0 10px 15px -3px rgba(0, 0, 0, 0.08), 0 4px 6px -4px rgba(0, 0, 0, 0.04)",
  "shadow-xl": "0 20px 25px -5px rgba(0, 0, 0, 0.08), 0 8px 10px -6px rgba(0, 0, 0, 0.04)",
};

/** Dark-theme shadow tokens (slightly more opaque for visibility on dark backgrounds) */
const darkShadows: Record<string, string> = {
  "shadow-sm": "0 1px 2px rgba(0, 0, 0, 0.2)",
  "shadow-md": "0 4px 6px -1px rgba(0, 0, 0, 0.3), 0 2px 4px -2px rgba(0, 0, 0, 0.2)",
  "shadow-lg": "0 10px 15px -3px rgba(0, 0, 0, 0.35), 0 4px 6px -4px rgba(0, 0, 0, 0.2)",
  "shadow-xl": "0 20px 25px -5px rgba(0, 0, 0, 0.4), 0 8px 10px -6px rgba(0, 0, 0, 0.25)",
};

/** Merge shared tokens + shadow set into a theme's color vars */
function withTokens(
  colorVars: Record<string, string>,
  scheme: "dark" | "light",
): Record<string, string> {
  return {
    ...sharedTokens,
    ...(scheme === "dark" ? darkShadows : lightShadows),
    ...colorVars,
  };
}

export const themes: ThemeDefinition[] = [
  // ── Clean ───────────────────────────────────────────────
  {
    id: "clean-light",
    label: "Clean Light",
    group: "branded",
    colorScheme: "light",
    shikiTheme: "github-light",
    vars: withTokens(
      {
        bg: "#ffffff",
        "bg-secondary": "#fafafa",
        "bg-tertiary": "#f4f4f5",
        border: "#e4e4e7",
        text: "#18181b",
        "text-secondary": "#71717a",
        accent: "#6366f1",
        "accent-hover": "#4f46e5",
        success: "#22c55e",
        danger: "#ef4444",
        warning: "#f59e0b",
        "trigger-color": "#f59e0b",
        "source-color": "#06b6d4",
        "executor-color": "#8b5cf6",
        "sink-color": "#22c55e",
        "primary-foreground": "#ffffff",
      },
      "light",
    ),
  },
  {
    id: "clean-dark",
    label: "Clean Dark",
    group: "branded",
    colorScheme: "dark",
    shikiTheme: "github-dark",
    vars: withTokens(
      {
        bg: "#09090b",
        "bg-secondary": "#18181b",
        "bg-tertiary": "#27272a",
        border: "#3f3f46",
        text: "#fafafa",
        "text-secondary": "#a1a1aa",
        accent: "#818cf8",
        "accent-hover": "#6366f1",
        success: "#22c55e",
        danger: "#ef4444",
        warning: "#f59e0b",
        "trigger-color": "#f59e0b",
        "source-color": "#06b6d4",
        "executor-color": "#a78bfa",
        "sink-color": "#22c55e",
        "primary-foreground": "#09090b",
      },
      "dark",
    ),
  },
  // ── Branded ──────────────────────────────────────────────
  {
    id: "eldritch",
    label: "Eldritch",
    group: "branded",
    colorScheme: "dark",
    shikiTheme: eldritchShikiTheme,
    vars: withTokens(toCssVarMap(eldritchDark), "dark"),
  },
  {
    id: "cosmic",
    label: "Cosmic",
    group: "branded",
    colorScheme: "dark",
    shikiTheme: "tokyo-night",
    vars: withTokens(
      {
        bg: "#110e20",
        "bg-secondary": "#1a1530",
        "bg-tertiary": "#252040",
        border: "#362e58",
        text: "#e0daf0",
        "text-secondary": "#8880a8",
        accent: "#b496ff",
        "accent-hover": "#9d7aff",
        success: "#5ae8a0",
        danger: "#ff5a6a",
        warning: "#e8c84a",
        "trigger-color": "#e8c84a",
        "source-color": "#5ab8ff",
        "executor-color": "#b496ff",
        "sink-color": "#5ae8a0",
        "primary-foreground": "#110e20",
      },
      "dark",
    ),
  },
  {
    id: "eldritch-light",
    label: "Eldritch Light",
    group: "branded",
    colorScheme: "light",
    shikiTheme: eldritchLightShikiTheme,
    vars: withTokens(toCssVarMap(eldritchLight), "light"),
  },
  {
    id: "cosmic-light",
    label: "Cosmic Light",
    group: "branded",
    colorScheme: "light",
    shikiTheme: "github-light",
    vars: withTokens(
      {
        bg: "#f5f0fa",
        "bg-secondary": "#ffffff",
        "bg-tertiary": "#e6ddf0",
        border: "#c8b8e0",
        text: "#1a1028",
        "text-secondary": "#6a5a88",
        accent: "#7c4dcc",
        "accent-hover": "#6a3db8",
        success: "#1a8a50",
        danger: "#cc3344",
        warning: "#a88020",
        "trigger-color": "#a88020",
        "source-color": "#3070cc",
        "executor-color": "#7c4dcc",
        "sink-color": "#1a8a50",
        "primary-foreground": "#fff",
      },
      "light",
    ),
  },
  // ── Presets ──────────────────────────────────────────────
  {
    id: "nord",
    label: "Nord",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "nord",
    vars: withTokens(
      {
        bg: "#2e3440",
        "bg-secondary": "#3b4252",
        "bg-tertiary": "#434c5e",
        border: "#4c566a",
        text: "#eceff4",
        "text-secondary": "#d8dee9",
        accent: "#88c0d0",
        "accent-hover": "#7ab3c3",
        success: "#a3be8c",
        danger: "#bf616a",
        warning: "#ebcb8b",
        "trigger-color": "#ebcb8b",
        "source-color": "#88c0d0",
        "executor-color": "#b48ead",
        "sink-color": "#a3be8c",
        "primary-foreground": "#2e3440",
      },
      "dark",
    ),
  },
  {
    id: "monokai",
    label: "Monokai",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "monokai",
    vars: withTokens(
      {
        bg: "#272822",
        "bg-secondary": "#1e1f1c",
        "bg-tertiary": "#3e3d32",
        border: "#49483e",
        text: "#f8f8f2",
        "text-secondary": "#a6a699",
        accent: "#66d9ef",
        "accent-hover": "#52c5db",
        success: "#a6e22e",
        danger: "#f92672",
        warning: "#e6db74",
        "trigger-color": "#e6db74",
        "source-color": "#66d9ef",
        "executor-color": "#ae81ff",
        "sink-color": "#a6e22e",
        "primary-foreground": "#272822",
      },
      "dark",
    ),
  },
  {
    id: "solarized-dark",
    label: "Solarized Dark",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "solarized-dark",
    vars: withTokens(
      {
        bg: "#002b36",
        "bg-secondary": "#073642",
        "bg-tertiary": "#0a4050",
        border: "#586e75",
        text: "#fdf6e3",
        "text-secondary": "#93a1a1",
        accent: "#268bd2",
        "accent-hover": "#1a7bbe",
        success: "#859900",
        danger: "#dc322f",
        warning: "#b58900",
        "trigger-color": "#b58900",
        "source-color": "#268bd2",
        "executor-color": "#6c71c4",
        "sink-color": "#859900",
        "primary-foreground": "#fdf6e3",
      },
      "dark",
    ),
  },
  {
    id: "solarized-light",
    label: "Solarized Light",
    group: "preset",
    colorScheme: "light",
    shikiTheme: "solarized-light",
    vars: withTokens(
      {
        bg: "#fdf6e3",
        "bg-secondary": "#eee8d5",
        "bg-tertiary": "#e0dbc8",
        border: "#93a1a1",
        text: "#073642",
        "text-secondary": "#586e75",
        accent: "#268bd2",
        "accent-hover": "#1a7bbe",
        success: "#859900",
        danger: "#dc322f",
        warning: "#b58900",
        "trigger-color": "#b58900",
        "source-color": "#268bd2",
        "executor-color": "#6c71c4",
        "sink-color": "#859900",
        "primary-foreground": "#fdf6e3",
      },
      "light",
    ),
  },
  {
    id: "dracula",
    label: "Dracula",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "dracula",
    vars: withTokens(
      {
        bg: "#282a36",
        "bg-secondary": "#21222c",
        "bg-tertiary": "#343746",
        border: "#44475a",
        text: "#f8f8f2",
        "text-secondary": "#6272a4",
        accent: "#8be9fd",
        "accent-hover": "#6dd5e9",
        success: "#50fa7b",
        danger: "#ff5555",
        warning: "#f1fa8c",
        "trigger-color": "#f1fa8c",
        "source-color": "#8be9fd",
        "executor-color": "#bd93f9",
        "sink-color": "#50fa7b",
        "primary-foreground": "#282a36",
      },
      "dark",
    ),
  },
  {
    id: "harry-potter",
    label: "Harry Potter",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "tokyo-night",
    vars: withTokens(
      {
        bg: "#0a0a0f",
        "bg-secondary": "#12121a",
        "bg-tertiary": "#1a1a25",
        border: "rgba(201,168,76,0.25)",
        text: "#f5e6c8",
        "text-secondary": "#8a7a60",
        accent: "#c9a84c",
        "accent-hover": "#b89840",
        success: "#4ade80",
        danger: "#f87171",
        warning: "#f0c060",
        "trigger-color": "#e8622a",
        "source-color": "#2aace8",
        "executor-color": "#a855f7",
        "sink-color": "#4ade80",
        "primary-foreground": "#0a0a0f",
      },
      "dark",
    ),
  },
];

export function getDefaultThemeId(): string {
  return localStorage.getItem(STORAGE_KEY) || "clean-light";
}

export function findTheme(id: string): ThemeDefinition {
  return themes.find((t) => t.id === id) || themes[0];
}
