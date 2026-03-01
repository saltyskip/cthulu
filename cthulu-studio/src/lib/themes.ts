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

export const themes: ThemeDefinition[] = [
  // ── Branded ──────────────────────────────────────────────
  {
    id: "eldritch",
    label: "Eldritch",
    group: "branded",
    colorScheme: "dark",
    shikiTheme: eldritchShikiTheme,
    vars: toCssVarMap(eldritchDark),
  },
  {
    id: "cosmic",
    label: "Cosmic",
    group: "branded",
    colorScheme: "dark",
    shikiTheme: "tokyo-night",
    vars: {
      bg: "#110e20",
      "bg-secondary": "#1a1530",
      "bg-tertiary": "#252040",
      border: "#362e58",
      text: "#e0daf0",
      "text-secondary": "#8880a8",
      accent: "#b496ff",
      success: "#5ae8a0",
      danger: "#ff5a6a",
      warning: "#e8c84a",
      "trigger-color": "#e8c84a",
      "source-color": "#5ab8ff",
      "executor-color": "#b496ff",
      "sink-color": "#5ae8a0",
      "primary-foreground": "#110e20",
    },
  },
  {
    id: "eldritch-light",
    label: "Eldritch Light",
    group: "branded",
    colorScheme: "light",
    shikiTheme: eldritchLightShikiTheme,
    vars: toCssVarMap(eldritchLight),
  },
  {
    id: "cosmic-light",
    label: "Cosmic Light",
    group: "branded",
    colorScheme: "light",
    shikiTheme: "github-light",
    vars: {
      bg: "#f5f0fa",
      "bg-secondary": "#ffffff",
      "bg-tertiary": "#e6ddf0",
      border: "#c8b8e0",
      text: "#1a1028",
      "text-secondary": "#6a5a88",
      accent: "#7c4dcc",
      success: "#1a8a50",
      danger: "#cc3344",
      warning: "#a88020",
      "trigger-color": "#a88020",
      "source-color": "#3070cc",
      "executor-color": "#7c4dcc",
      "sink-color": "#1a8a50",
      "primary-foreground": "#fff",
    },
  },
  // ── Presets ──────────────────────────────────────────────
  {
    id: "nord",
    label: "Nord",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "nord",
    vars: {
      bg: "#2e3440",
      "bg-secondary": "#3b4252",
      "bg-tertiary": "#434c5e",
      border: "#4c566a",
      text: "#eceff4",
      "text-secondary": "#d8dee9",
      accent: "#88c0d0",
      success: "#a3be8c",
      danger: "#bf616a",
      warning: "#ebcb8b",
      "trigger-color": "#ebcb8b",
      "source-color": "#88c0d0",
      "executor-color": "#b48ead",
      "sink-color": "#a3be8c",
      "primary-foreground": "#2e3440",
    },
  },
  {
    id: "monokai",
    label: "Monokai",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "monokai",
    vars: {
      bg: "#272822",
      "bg-secondary": "#1e1f1c",
      "bg-tertiary": "#3e3d32",
      border: "#49483e",
      text: "#f8f8f2",
      "text-secondary": "#a6a699",
      accent: "#66d9ef",
      success: "#a6e22e",
      danger: "#f92672",
      warning: "#e6db74",
      "trigger-color": "#e6db74",
      "source-color": "#66d9ef",
      "executor-color": "#ae81ff",
      "sink-color": "#a6e22e",
      "primary-foreground": "#272822",
    },
  },
  {
    id: "solarized-dark",
    label: "Solarized Dark",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "solarized-dark",
    vars: {
      bg: "#002b36",
      "bg-secondary": "#073642",
      "bg-tertiary": "#0a4050",
      border: "#586e75",
      text: "#fdf6e3",
      "text-secondary": "#93a1a1",
      accent: "#268bd2",
      success: "#859900",
      danger: "#dc322f",
      warning: "#b58900",
      "trigger-color": "#b58900",
      "source-color": "#268bd2",
      "executor-color": "#6c71c4",
      "sink-color": "#859900",
      "primary-foreground": "#fdf6e3",
    },
  },
  {
    id: "solarized-light",
    label: "Solarized Light",
    group: "preset",
    colorScheme: "light",
    shikiTheme: "solarized-light",
    vars: {
      bg: "#fdf6e3",
      "bg-secondary": "#eee8d5",
      "bg-tertiary": "#e0dbc8",
      border: "#93a1a1",
      text: "#073642",
      "text-secondary": "#586e75",
      accent: "#268bd2",
      success: "#859900",
      danger: "#dc322f",
      warning: "#b58900",
      "trigger-color": "#b58900",
      "source-color": "#268bd2",
      "executor-color": "#6c71c4",
      "sink-color": "#859900",
      "primary-foreground": "#fdf6e3",
    },
  },
  {
    id: "dracula",
    label: "Dracula",
    group: "preset",
    colorScheme: "dark",
    shikiTheme: "dracula",
    vars: {
      bg: "#282a36",
      "bg-secondary": "#21222c",
      "bg-tertiary": "#343746",
      border: "#44475a",
      text: "#f8f8f2",
      "text-secondary": "#6272a4",
      accent: "#8be9fd",
      success: "#50fa7b",
      danger: "#ff5555",
      warning: "#f1fa8c",
      "trigger-color": "#f1fa8c",
      "source-color": "#8be9fd",
      "executor-color": "#bd93f9",
      "sink-color": "#50fa7b",
      "primary-foreground": "#282a36",
    },
  },
];

export function getDefaultThemeId(): string {
  return localStorage.getItem(STORAGE_KEY) || "eldritch";
}

export function findTheme(id: string): ThemeDefinition {
  return themes.find((t) => t.id === id) || themes[0];
}
