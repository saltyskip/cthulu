/**
 * Custom Shiki theme for the Harry Potter palette.
 *
 * Color story:
 *   Parchment & ink (bg, text)     → ancient manuscript tones
 *   Old gold (keywords, accents)   → Gryffindor gold, torch light
 *   Bright gold (functions)        → wand sparks, patronus glow
 *   Slytherin green (strings)      → potion ingredients, forest
 *   Dumbledore purple (types)      → robes, amethyst crystals
 *   Ember orange (numbers)         → floo powder, fireplace coals
 *   Bronze (operators, comments)   → Hufflepuff warmth, antique
 *   Ravenclaw blue (properties)    → wit, wisdom, starlight
 */
export const hpShikiTheme = {
  name: "harry-potter",
  type: "dark" as const,
  colors: {
    "editor.background": "#0a0a0f",
    "editor.foreground": "#f5e6c8",
    "editor.lineHighlightBackground": "#12100a",
    "editor.selectionBackground": "#c9a84c30",
    "editorCursor.foreground": "#c9a84c",
    "editorLineNumber.foreground": "#4a3f2a",
    "editorLineNumber.activeForeground": "#8a7a60",
    "editorIndentGuide.background": "#1a1510",
    "editorBracketMatch.background": "#c9a84c18",
    "editorBracketMatch.border": "#c9a84c60",
  },
  tokenColors: [
    // ── Comments — faded ink, receding into parchment ──
    {
      scope: ["comment", "punctuation.definition.comment"],
      settings: { foreground: "#6a5a40", fontStyle: "italic" },
    },
    // ── Keywords / control flow — old gold, torch light ──
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
      settings: { foreground: "#c9a84c" },
    },
    // ── Operators / punctuation — bronze, subtle ──
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
      settings: { foreground: "#8a7a60" },
    },
    // ── Functions — wand sparks, bright gold ──
    {
      scope: [
        "entity.name.function",
        "meta.function-call",
        "support.function",
        "entity.name.method",
      ],
      settings: { foreground: "#f0c060" },
    },
    // ── Strings — Slytherin green, potions ──
    {
      scope: [
        "string",
        "string.quoted",
        "string.template",
      ],
      settings: { foreground: "#5bb98c" },
    },
    // ── String interpolation — gold flash ──
    {
      scope: [
        "punctuation.definition.template-expression",
        "punctuation.section.embedded",
      ],
      settings: { foreground: "#c9a84c" },
    },
    // ── Types / classes — Dumbledore's robes, purple ──
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
      settings: { foreground: "#a87ed8" },
    },
    // ── Type parameters / generics ──
    {
      scope: ["variable.other.type", "entity.name.type.parameter"],
      settings: { foreground: "#b890e0" },
    },
    // ── Numbers / constants — ember orange, floo powder ──
    {
      scope: [
        "constant.numeric",
        "constant.language",
        "constant.language.boolean",
        "constant.language.null",
        "constant.language.undefined",
      ],
      settings: { foreground: "#e8622a" },
    },
    // ── Constants (other) — warm gold ──
    {
      scope: ["variable.other.constant", "variable.other.enummember"],
      settings: { foreground: "#daa850" },
    },
    // ── Variables — parchment, clean ──
    {
      scope: ["variable", "variable.other", "variable.parameter"],
      settings: { foreground: "#f5e6c8" },
    },
    // ── Properties / fields — Ravenclaw steel blue ──
    {
      scope: [
        "variable.other.property",
        "variable.other.object.property",
        "support.variable.property",
        "meta.object-literal.key",
      ],
      settings: { foreground: "#5b7fb8" },
    },
    // ── Regex / escape sequences — ember red ──
    {
      scope: [
        "string.regexp",
        "constant.character.escape",
        "constant.other.character-class.regexp",
      ],
      settings: { foreground: "#f87171" },
    },
    // ── Tags (HTML/JSX) — gold ──
    {
      scope: [
        "entity.name.tag",
        "punctuation.definition.tag",
        "support.class.component",
      ],
      settings: { foreground: "#c9a84c" },
    },
    // ── Attributes — bright gold ──
    {
      scope: [
        "entity.other.attribute-name",
      ],
      settings: { foreground: "#f0c060" },
    },
    // ── CSS ──
    {
      scope: ["support.type.property-name.css", "support.type.vendored.property-name.css"],
      settings: { foreground: "#5b7fb8" },
    },
    {
      scope: ["support.constant.property-value.css", "constant.other.color.rgb-value.hex.css"],
      settings: { foreground: "#e8622a" },
    },
    {
      scope: ["entity.other.attribute-name.class.css", "entity.other.attribute-name.id.css"],
      settings: { foreground: "#f0c060" },
    },
    // ── JSON keys ──
    {
      scope: ["support.type.property-name.json"],
      settings: { foreground: "#5b7fb8" },
    },
    // ── Markdown ──
    {
      scope: ["markup.heading", "entity.name.section.markdown"],
      settings: { foreground: "#c9a84c", fontStyle: "bold" },
    },
    {
      scope: ["markup.bold"],
      settings: { foreground: "#f0c060", fontStyle: "bold" },
    },
    {
      scope: ["markup.italic"],
      settings: { foreground: "#a87ed8", fontStyle: "italic" },
    },
    {
      scope: ["markup.inline.raw", "markup.fenced_code.block"],
      settings: { foreground: "#5bb98c" },
    },
    {
      scope: ["markup.list.unnumbered", "markup.list.numbered"],
      settings: { foreground: "#e8622a" },
    },
    // ── Imports / modules ──
    {
      scope: ["meta.import", "keyword.control.import", "keyword.control.export"],
      settings: { foreground: "#c9a84c" },
    },
    {
      scope: ["variable.other.readwrite.alias"],
      settings: { foreground: "#f5e6c8" },
    },
    // ── this / self ──
    {
      scope: ["variable.language.this", "variable.language.self", "variable.language.super"],
      settings: { foreground: "#c9a84c", fontStyle: "italic" },
    },
    // ── Decorators ──
    {
      scope: ["meta.decorator", "punctuation.decorator"],
      settings: { foreground: "#a87ed8" },
    },
    // ── Rust-specific ──
    {
      scope: ["entity.name.lifetime.rust"],
      settings: { foreground: "#e8622a", fontStyle: "italic" },
    },
    {
      scope: ["keyword.operator.macro.rust", "entity.name.function.macro.rust"],
      settings: { foreground: "#f0c060", fontStyle: "bold" },
    },
  ],
};
