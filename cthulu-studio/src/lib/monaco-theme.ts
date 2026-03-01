import type { ThemeDefinition } from "./themes";

interface MonacoTokenRule {
  token: string;
  foreground?: string;
  fontStyle?: string;
}

/**
 * Semantic color roles for syntax highlighting.
 * Built from either a custom Shiki theme or derived from theme vars.
 */
interface TokenPalette {
  property?: string;
  string?: string;
  number?: string;
  keyword?: string;
  comment?: string;
  type?: string;
  function?: string;
  variable?: string;
  operator?: string;
  tag?: string;
  attribute?: string;
}

/**
 * Extract a TokenPalette from a custom Shiki theme's tokenColors.
 */
function paletteFromShikiTheme(shikiTheme: Record<string, unknown>): TokenPalette {
  const tokenColors = shikiTheme.tokenColors as
    | Array<{ scope?: string | string[]; settings?: { foreground?: string } }>
    | undefined;
  if (!tokenColors) return {};

  const colorMap: Record<string, string> = {};
  for (const entry of tokenColors) {
    if (!entry.scope || !entry.settings?.foreground) continue;
    const scopes = Array.isArray(entry.scope) ? entry.scope : [entry.scope];
    for (const scope of scopes) {
      colorMap[scope] = entry.settings.foreground.replace("#", "");
    }
  }

  const find = (...scopes: string[]): string | undefined => {
    for (const s of scopes) if (colorMap[s]) return colorMap[s];
    return undefined;
  };

  return {
    property: find("variable.other.property", "meta.object-literal.key", "support.type.property-name.json"),
    string: find("string", "string.quoted"),
    number: find("constant.numeric"),
    keyword: find("keyword", "keyword.control"),
    comment: find("comment"),
    type: find("entity.name.type", "support.type"),
    function: find("entity.name.function", "support.function"),
    variable: find("variable", "variable.other"),
    operator: find("keyword.operator"),
    tag: find("entity.name.tag"),
    attribute: find("entity.other.attribute-name"),
  };
}

/**
 * Derive a TokenPalette from theme CSS vars.
 * Maps semantic roles to the closest theme colors.
 */
function paletteFromVars(v: Record<string, string>): TokenPalette {
  const strip = (hex: string) => hex.replace("#", "");
  return {
    property: strip(v["source-color"] ?? v["accent"]),
    string: strip(v["success"]),
    number: strip(v["warning"]),
    keyword: strip(v["accent"]),
    comment: strip(v["text-secondary"]),
    type: strip(v["executor-color"]),
    function: strip(v["warning"]),
    variable: strip(v["text"]),
    operator: strip(v["text-secondary"]),
    tag: strip(v["accent"]),
    attribute: strip(v["warning"]),
  };
}

/**
 * Build Monaco-native token rules from a TokenPalette.
 * Monaco's built-in tokenizers use token names like
 * `string.key.json`, `number.json`, `keyword.ts`.
 */
function buildMonacoNativeRules(palette: TokenPalette): MonacoTokenRule[] {
  const rules: MonacoTokenRule[] = [];
  const add = (token: string, fg?: string, fontStyle?: string) => {
    if (fg) rules.push({ token, foreground: fg, ...(fontStyle ? { fontStyle } : {}) });
  };

  // ── JSON ──
  add("string.key.json", palette.property);
  add("string.value.json", palette.string);
  add("number.json", palette.number);
  add("keyword.json", palette.keyword);

  // ── JavaScript / TypeScript ──
  for (const lang of ["ts", "js"]) {
    add(`keyword.${lang}`, palette.keyword);
    add(`string.${lang}`, palette.string);
    add(`number.${lang}`, palette.number);
    add(`comment.${lang}`, palette.comment);
    add(`identifier.${lang}`, palette.variable);
    add(`delimiter.${lang}`, palette.operator);
  }
  add("type.identifier.ts", palette.type);

  // ── HTML ──
  add("tag.html", palette.tag);
  add("attribute.name.html", palette.attribute);
  add("attribute.value.html", palette.string);
  add("string.html", palette.string);
  add("comment.html", palette.comment);

  // ── CSS ──
  add("attribute.name.css", palette.property);
  add("attribute.value.css", palette.string);
  add("number.css", palette.number);
  add("string.css", palette.string);
  add("keyword.css", palette.keyword);
  add("tag.css", palette.tag);

  // ── Markdown ──
  add("string.link.md", palette.string);
  add("keyword.md", palette.keyword);
  add("comment.md", palette.comment);

  // ── Generic fallbacks ──
  add("string", palette.string);
  add("number", palette.number);
  add("keyword", palette.keyword);
  add("comment", palette.comment);
  add("type", palette.type);
  add("identifier", palette.variable);
  add("delimiter", palette.operator);
  add("tag", palette.tag);
  add("attribute.name", palette.attribute);
  add("attribute.value", palette.string);
  add("function", palette.function);

  return rules;
}

/**
 * Convert Shiki TextMate tokenColors to Monaco rules (for TextMate-based tokenizers).
 */
function shikiToMonacoRules(shikiTheme: Record<string, unknown>): MonacoTokenRule[] {
  const tokenColors = shikiTheme.tokenColors as
    | Array<{ scope?: string | string[]; settings?: { foreground?: string; fontStyle?: string } }>
    | undefined;
  if (!tokenColors) return [];

  const rules: MonacoTokenRule[] = [];
  for (const entry of tokenColors) {
    if (!entry.scope || !entry.settings?.foreground) continue;
    const scopes = Array.isArray(entry.scope) ? entry.scope : [entry.scope];
    const fg = entry.settings.foreground.replace("#", "");
    for (const scope of scopes) {
      rules.push({
        token: scope,
        foreground: fg,
        ...(entry.settings.fontStyle ? { fontStyle: entry.settings.fontStyle } : {}),
      });
    }
  }
  return rules;
}

/**
 * Define and apply a Monaco editor theme derived from a ThemeDefinition.
 * Works for both custom Shiki themes (full token colors) and bundled
 * string themes (derives token colors from the theme's CSS vars).
 */
export function applyMonacoTheme(
  monaco: { editor: { defineTheme: Function; setTheme: Function } },
  theme: ThemeDefinition,
) {
  const v = theme.vars;
  const base = theme.colorScheme === "dark" ? "vs-dark" : "vs";
  const selectionBg = theme.colorScheme === "dark" ? "#264f5a88" : "#add6ff";
  const isCustom = typeof theme.shikiTheme === "object";

  // Build token palette from either custom Shiki theme or CSS vars
  const palette = isCustom
    ? paletteFromShikiTheme(theme.shikiTheme as Record<string, unknown>)
    : paletteFromVars(v);

  // TextMate rules only available from custom Shiki themes
  const textmateRules = isCustom
    ? shikiToMonacoRules(theme.shikiTheme as Record<string, unknown>)
    : [];

  const rules = [...textmateRules, ...buildMonacoNativeRules(palette)];

  // Editor chrome colors from custom Shiki theme (if available)
  const shikiColors = isCustom
    ? (theme.shikiTheme as Record<string, unknown>).colors as Record<string, string> | undefined
    : undefined;

  monaco.editor.defineTheme("cthulu-active", {
    base,
    inherit: true,
    rules,
    colors: {
      "editor.background": v["bg"],
      "editor.foreground": v["text"],
      "editorLineNumber.foreground": shikiColors?.["editorLineNumber.foreground"] ?? v["text-secondary"],
      "editorLineNumber.activeForeground": shikiColors?.["editorLineNumber.activeForeground"] ?? v["text"],
      "editor.selectionBackground": shikiColors?.["editor.selectionBackground"] ?? selectionBg,
      "editor.lineHighlightBackground": shikiColors?.["editor.lineHighlightBackground"] ?? v["bg-secondary"],
      "editorCursor.foreground": shikiColors?.["editorCursor.foreground"] ?? v["accent"],
      "editorIndentGuide.background": shikiColors?.["editorIndentGuide.background"] ?? v["bg-tertiary"],
      "editorWidget.background": v["bg-secondary"],
      "editorWidget.border": v["border"],
      "editorBracketMatch.background": shikiColors?.["editorBracketMatch.background"] ?? "#264f5530",
      "editorBracketMatch.border": shikiColors?.["editorBracketMatch.border"] ?? v["accent"] + "80",
      "input.background": v["bg"],
      "input.border": v["border"],
      "list.hoverBackground": v["bg-secondary"],
      "list.activeSelectionBackground": selectionBg,
    },
  });
  monaco.editor.setTheme("cthulu-active");
}
