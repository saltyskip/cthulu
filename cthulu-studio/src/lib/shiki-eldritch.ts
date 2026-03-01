/**
 * Custom Shiki theme for the Eldritch palette.
 *
 * Color story:
 *   Deep ocean (bg, comments)    → dark teals and grays
 *   Bioluminescence (keywords)   → bright teal-cyan
 *   Coral reef (strings, values) → warm greens
 *   Anglerfish lure (functions)  → amber gold
 *   Jellyfish (types, classes)   → soft purple
 *   Sea foam (properties)        → light blue-teal
 */
export const eldritchShikiTheme = {
  name: "eldritch",
  type: "dark" as const,
  colors: {
    "editor.background": "#0d1a20",
    "editor.foreground": "#d1e1e8",
    "editor.lineHighlightBackground": "#12222a",
    "editor.selectionBackground": "#264f5a88",
    "editorCursor.foreground": "#4ec9b0",
    "editorLineNumber.foreground": "#3a5560",
    "editorLineNumber.activeForeground": "#7a9baa",
    "editorIndentGuide.background": "#1e3038",
    "editorBracketMatch.background": "#2a4a5530",
    "editorBracketMatch.border": "#4ec9b080",
  },
  tokenColors: [
    // ── Comments — deep water, recedes ──
    {
      scope: ["comment", "punctuation.definition.comment"],
      settings: { foreground: "#506a78", fontStyle: "italic" },
    },
    // ── Keywords / control flow — bioluminescent teal ──
    {
      scope: [
        "keyword",
        "keyword.control",
        "keyword.operator.expression",
        "keyword.operator.new",
        "keyword.operator.logical",
        "storage.type",
        "storage.modifier",
      ],
      settings: { foreground: "#4ec9b0" },
    },
    // ── Operators / punctuation — muted but clear ──
    {
      scope: [
        "keyword.operator",
        "keyword.operator.assignment",
        "keyword.operator.arithmetic",
        "keyword.operator.comparison",
        "punctuation",
        "punctuation.separator",
        "punctuation.terminator",
        "meta.brace",
      ],
      settings: { foreground: "#8aa4b0" },
    },
    // ── Functions — anglerfish amber ──
    {
      scope: [
        "entity.name.function",
        "meta.function-call",
        "support.function",
        "entity.name.method",
      ],
      settings: { foreground: "#deb06a" },
    },
    // ── Strings — sea plants, warm green ──
    {
      scope: [
        "string",
        "string.quoted",
        "string.template",
      ],
      settings: { foreground: "#8ed4a8" },
    },
    // ── String interpolation — brighter ──
    {
      scope: [
        "punctuation.definition.template-expression",
        "punctuation.section.embedded",
      ],
      settings: { foreground: "#4ec9b0" },
    },
    // ── Types / classes / interfaces — jellyfish purple ──
    {
      scope: [
        "entity.name.type",
        "entity.name.class",
        "support.type",
        "support.class",
        "entity.other.inherited-class",
        "storage.type.interface",
        "storage.type.type",
      ],
      settings: { foreground: "#b89ce0" },
    },
    // ── Type parameters / generics ──
    {
      scope: ["variable.other.type", "entity.name.type.parameter"],
      settings: { foreground: "#c4aeea" },
    },
    // ── Numbers / constants / booleans — warm coral-orange ──
    {
      scope: [
        "constant.numeric",
        "constant.language",
        "constant.language.boolean",
        "constant.language.null",
        "constant.language.undefined",
      ],
      settings: { foreground: "#f0a070" },
    },
    // ── Constants (other) ──
    {
      scope: ["variable.other.constant", "variable.other.enummember"],
      settings: { foreground: "#e0c08a" },
    },
    // ── Variables — main text, clean ──
    {
      scope: ["variable", "variable.other", "variable.parameter"],
      settings: { foreground: "#d1e1e8" },
    },
    // ── Properties / fields — warm sand ──
    {
      scope: [
        "variable.other.property",
        "variable.other.object.property",
        "support.variable.property",
        "meta.object-literal.key",
      ],
      settings: { foreground: "#e0c08a" },
    },
    // ── Regex / escape sequences — coral red ──
    {
      scope: [
        "string.regexp",
        "constant.character.escape",
        "constant.other.character-class.regexp",
      ],
      settings: { foreground: "#f07068" },
    },
    // ── Tags (HTML/JSX) — teal ──
    {
      scope: [
        "entity.name.tag",
        "punctuation.definition.tag",
        "support.class.component",
      ],
      settings: { foreground: "#4ec9b0" },
    },
    // ── Attributes — amber ──
    {
      scope: [
        "entity.other.attribute-name",
      ],
      settings: { foreground: "#deb06a" },
    },
    // ── CSS ──
    {
      scope: ["support.type.property-name.css", "support.type.vendored.property-name.css"],
      settings: { foreground: "#e0c08a" },
    },
    {
      scope: ["support.constant.property-value.css", "constant.other.color.rgb-value.hex.css"],
      settings: { foreground: "#f0a070" },
    },
    {
      scope: ["entity.other.attribute-name.class.css", "entity.other.attribute-name.id.css"],
      settings: { foreground: "#deb06a" },
    },
    // ── JSON keys ──
    {
      scope: ["support.type.property-name.json"],
      settings: { foreground: "#e0c08a" },
    },
    // ── Markdown ──
    {
      scope: ["markup.heading", "entity.name.section.markdown"],
      settings: { foreground: "#4ec9b0", fontStyle: "bold" },
    },
    {
      scope: ["markup.bold"],
      settings: { foreground: "#deb06a", fontStyle: "bold" },
    },
    {
      scope: ["markup.italic"],
      settings: { foreground: "#b89ce0", fontStyle: "italic" },
    },
    {
      scope: ["markup.inline.raw", "markup.fenced_code.block"],
      settings: { foreground: "#8ed4a8" },
    },
    {
      scope: ["markup.list.unnumbered", "markup.list.numbered"],
      settings: { foreground: "#f0a070" },
    },
    // ── Imports / modules ──
    {
      scope: ["meta.import", "keyword.control.import", "keyword.control.export"],
      settings: { foreground: "#4ec9b0" },
    },
    {
      scope: ["variable.other.readwrite.alias"],
      settings: { foreground: "#d1e1e8" },
    },
    // ── this / self ──
    {
      scope: ["variable.language.this", "variable.language.self", "variable.language.super"],
      settings: { foreground: "#4ec9b0", fontStyle: "italic" },
    },
    // ── Decorators ──
    {
      scope: ["meta.decorator", "punctuation.decorator"],
      settings: { foreground: "#b89ce0" },
    },
    // ── Rust-specific ──
    {
      scope: ["entity.name.lifetime.rust"],
      settings: { foreground: "#f0a070", fontStyle: "italic" },
    },
    {
      scope: ["keyword.operator.macro.rust", "entity.name.function.macro.rust"],
      settings: { foreground: "#deb06a", fontStyle: "bold" },
    },
  ],
};
