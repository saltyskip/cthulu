/**
 * Cthulu brand palette — single source of truth.
 *
 * Both cthulu-studio and cthulu-site import from here.
 * Each app applies these values through its own theming layer
 * (ThemeProvider for Studio, CSS vars / @theme for Site).
 */

export interface BrandPalette {
  bg: string;
  bgSecondary: string;
  bgTertiary: string;
  border: string;
  text: string;
  textSecondary: string;
  accent: string;
  success: string;
  danger: string;
  warning: string;
  triggerColor: string;
  sourceColor: string;
  executorColor: string;
  sinkColor: string;
  primaryForeground: string;
}

/** Eldritch Dark — deep ocean, bioluminescent accents */
export const eldritchDark: BrandPalette = {
  bg: "#0b1317",
  bgSecondary: "#111e25",
  bgTertiary: "#182b34",
  border: "#24404c",
  text: "#d1e1e8",
  textSecondary: "#7a9baa",
  accent: "#4ec9b0",
  success: "#5bb98c",
  danger: "#f07068",
  warning: "#daa850",
  triggerColor: "#daa850",
  sourceColor: "#4ec9b0",
  executorColor: "#9d8ce0",
  sinkColor: "#5bb98c",
  primaryForeground: "#0b1317",
};

/** Eldritch Light — morning fog, teal accents */
export const eldritchLight: BrandPalette = {
  bg: "#f7f9f8",
  bgSecondary: "#ffffff",
  bgTertiary: "#eaefed",
  border: "#c8d4d0",
  text: "#1a2a28",
  textSecondary: "#5a7a72",
  accent: "#0c8c72",
  success: "#167a4a",
  danger: "#c04040",
  warning: "#9a7018",
  triggerColor: "#9a7018",
  sourceColor: "#0c8c72",
  executorColor: "#6a4daa",
  sinkColor: "#167a4a",
  primaryForeground: "#ffffff",
};

/**
 * Convert a BrandPalette to the CSS variable map format used by Studio's ThemeProvider.
 * Keys use kebab-case with `--` prefix stripped (e.g. "bg-secondary").
 */
export function toCssVarMap(p: BrandPalette): Record<string, string> {
  return {
    bg: p.bg,
    "bg-secondary": p.bgSecondary,
    "bg-tertiary": p.bgTertiary,
    border: p.border,
    text: p.text,
    "text-secondary": p.textSecondary,
    accent: p.accent,
    success: p.success,
    danger: p.danger,
    warning: p.warning,
    "trigger-color": p.triggerColor,
    "source-color": p.sourceColor,
    "executor-color": p.executorColor,
    "sink-color": p.sinkColor,
    "primary-foreground": p.primaryForeground,
  };
}
