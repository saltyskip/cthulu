/**
 * Custom Shiki light theme for the Eldritch palette.
 *
 * Same color story as dark, adapted for light backgrounds:
 *   Deep ocean ink (text)          → near-black with cool tint
 *   Bioluminescence (keywords)     → darkened teal
 *   Coral reef (strings)           → forest green
 *   Anglerfish lure (functions)    → burnt amber
 *   Jellyfish (types)              → deep violet
 *   Warm sand (properties)         → dark gold
 */
export const eldritchLightShikiTheme = {
  name: "eldritch-light",
  type: "light" as const,
  colors: {
    "editor.background": "#f7f9f8",
    "editor.foreground": "#1a2a28",
    "editor.lineHighlightBackground": "#eef2f0",
    "editor.selectionBackground": "#0c8c7230",
    "editorCursor.foreground": "#0c8c72",
    "editorLineNumber.foreground": "#98aaa4",
    "editorLineNumber.activeForeground": "#5a7a72",
    "editorIndentGuide.background": "#dce4e0",
    "editorBracketMatch.background": "#0c8c7220",
    "editorBracketMatch.border": "#0c8c7260",
  },
  tokenColors: [
    // ── Comments — recedes into background ──
    {
      scope: ["comment", "punctuation.definition.comment"],
      settings: { foreground: "#8a9e98", fontStyle: "italic" },
    },
    // ── Keywords / control flow — deep teal ──
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
      settings: { foreground: "#0c8c72" },
    },
    // ── Operators / punctuation — steel gray ──
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
      settings: { foreground: "#5a7a72" },
    },
    // ── Functions — burnt amber ──
    {
      scope: [
        "entity.name.function",
        "meta.function-call",
        "support.function",
        "entity.name.method",
      ],
      settings: { foreground: "#a06818" },
    },
    // ── Strings — forest green ──
    {
      scope: [
        "string",
        "string.quoted",
        "string.template",
      ],
      settings: { foreground: "#2a7a48" },
    },
    // ── String interpolation ──
    {
      scope: [
        "punctuation.definition.template-expression",
        "punctuation.section.embedded",
      ],
      settings: { foreground: "#0c8c72" },
    },
    // ── Types / classes — deep violet ──
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
      settings: { foreground: "#6a4daa" },
    },
    // ── Type parameters ──
    {
      scope: ["variable.other.type", "entity.name.type.parameter"],
      settings: { foreground: "#7a5cc0" },
    },
    // ── Numbers / constants — terracotta ──
    {
      scope: [
        "constant.numeric",
        "constant.language",
        "constant.language.boolean",
        "constant.language.null",
        "constant.language.undefined",
      ],
      settings: { foreground: "#c05828" },
    },
    // ── Constants (other) — dark gold ──
    {
      scope: ["variable.other.constant", "variable.other.enummember"],
      settings: { foreground: "#8a6a18" },
    },
    // ── Variables — deep ink ──
    {
      scope: ["variable", "variable.other", "variable.parameter"],
      settings: { foreground: "#1a2a28" },
    },
    // ── Properties / fields — dark gold (warm, distinct from green strings) ──
    {
      scope: [
        "variable.other.property",
        "variable.other.object.property",
        "support.variable.property",
        "meta.object-literal.key",
      ],
      settings: { foreground: "#8a6a18" },
    },
    // ── Regex / escape sequences — coral ──
    {
      scope: [
        "string.regexp",
        "constant.character.escape",
        "constant.other.character-class.regexp",
      ],
      settings: { foreground: "#c04040" },
    },
    // ── Tags (HTML/JSX) — teal ──
    {
      scope: [
        "entity.name.tag",
        "punctuation.definition.tag",
        "support.class.component",
      ],
      settings: { foreground: "#0c8c72" },
    },
    // ── Attributes — amber ──
    {
      scope: ["entity.other.attribute-name"],
      settings: { foreground: "#a06818" },
    },
    // ── CSS ──
    {
      scope: ["support.type.property-name.css", "support.type.vendored.property-name.css"],
      settings: { foreground: "#8a6a18" },
    },
    {
      scope: ["support.constant.property-value.css", "constant.other.color.rgb-value.hex.css"],
      settings: { foreground: "#c05828" },
    },
    {
      scope: ["entity.other.attribute-name.class.css", "entity.other.attribute-name.id.css"],
      settings: { foreground: "#a06818" },
    },
    // ── JSON keys ──
    {
      scope: ["support.type.property-name.json"],
      settings: { foreground: "#8a6a18" },
    },
    // ── Markdown ──
    {
      scope: ["markup.heading", "entity.name.section.markdown"],
      settings: { foreground: "#0c8c72", fontStyle: "bold" },
    },
    {
      scope: ["markup.bold"],
      settings: { foreground: "#a06818", fontStyle: "bold" },
    },
    {
      scope: ["markup.italic"],
      settings: { foreground: "#6a4daa", fontStyle: "italic" },
    },
    {
      scope: ["markup.inline.raw", "markup.fenced_code.block"],
      settings: { foreground: "#2a7a48" },
    },
    // ── Imports ──
    {
      scope: ["meta.import", "keyword.control.import", "keyword.control.export"],
      settings: { foreground: "#0c8c72" },
    },
    // ── this / self ──
    {
      scope: ["variable.language.this", "variable.language.self", "variable.language.super"],
      settings: { foreground: "#0c8c72", fontStyle: "italic" },
    },
    // ── Decorators ──
    {
      scope: ["meta.decorator", "punctuation.decorator"],
      settings: { foreground: "#6a4daa" },
    },
  ],
};
