/**
 * Map file extensions to Nerd Font icons and colors.
 * Uses codepoints from Symbols Nerd Font Mono.
 */

interface FileIcon {
  icon: string;
  color: string;
}

const EXT_ICONS: Record<string, FileIcon> = {
  // TypeScript / JavaScript
  ts:         { icon: "\u{e628}", color: "#3178c6" },  //
  tsx:        { icon: "\u{e7ba}", color: "#3178c6" },  //
  js:         { icon: "\u{e74e}", color: "#f1e05a" },  //
  jsx:        { icon: "\u{e7ba}", color: "#f1e05a" },  //
  mjs:        { icon: "\u{e74e}", color: "#f1e05a" },
  cjs:        { icon: "\u{e74e}", color: "#f1e05a" },

  // Systems
  rs:         { icon: "\u{e7a8}", color: "#dea584" },  //
  go:         { icon: "\u{e627}", color: "#00add8" },  //
  c:          { icon: "\u{e61e}", color: "#555555" },  //
  cpp:        { icon: "\u{e61d}", color: "#f34b7d" },  //
  h:          { icon: "\u{e61e}", color: "#555555" },

  // Scripting
  py:         { icon: "\u{e73c}", color: "#3572a5" },  //
  rb:         { icon: "\u{e791}", color: "#cc342d" },  //
  sh:         { icon: "\u{e795}", color: "#89e051" },  //
  bash:       { icon: "\u{e795}", color: "#89e051" },
  zsh:        { icon: "\u{e795}", color: "#89e051" },
  fish:       { icon: "\u{e795}", color: "#89e051" },

  // JVM
  java:       { icon: "\u{e738}", color: "#b07219" },  //
  kt:         { icon: "\u{e634}", color: "#a97bff" },  //
  scala:      { icon: "\u{e737}", color: "#c22d40" },  //

  // Apple / Mobile
  swift:      { icon: "\u{e755}", color: "#f05138" },  //
  dart:       { icon: "\u{e798}", color: "#00b4ab" },  //

  // .NET
  cs:         { icon: "\u{f81a}", color: "#178600" },  //

  // Web
  html:       { icon: "\u{e736}", color: "#e34c26" },  //
  css:        { icon: "\u{e749}", color: "#563d7c" },  //
  scss:       { icon: "\u{e749}", color: "#c6538c" },
  less:       { icon: "\u{e749}", color: "#1d365d" },
  vue:        { icon: "\u{e6a0}", color: "#41b883" },  //
  svelte:     { icon: "\u{e697}", color: "#ff3e00" },  //

  // Data / Config
  json:       { icon: "\u{e60b}", color: "#cbcb41" },  //
  yaml:       { icon: "\u{e60b}", color: "#cb171e" },
  yml:        { icon: "\u{e60b}", color: "#cb171e" },
  toml:       { icon: "\u{e60b}", color: "#9c4221" },
  xml:        { icon: "\u{e619}", color: "#e37933" },  //

  // Docs
  md:         { icon: "\u{e73e}", color: "#519aba" },  //
  mdx:        { icon: "\u{e73e}", color: "#519aba" },
  txt:        { icon: "\u{f15c}", color: "#89898b" },  //

  // DevOps / Infra
  dockerfile: { icon: "\u{e7b0}", color: "#384d54" },  //
  docker:     { icon: "\u{e7b0}", color: "#384d54" },
  tf:         { icon: "\u{e69a}", color: "#5c4ee5" },

  // Database
  sql:        { icon: "\u{e706}", color: "#e38c00" },  //
  graphql:    { icon: "\u{e662}", color: "#e10098" },  //
  gql:        { icon: "\u{e662}", color: "#e10098" },

  // Images
  png:        { icon: "\u{f1c5}", color: "#a074c4" },  //
  jpg:        { icon: "\u{f1c5}", color: "#a074c4" },
  jpeg:       { icon: "\u{f1c5}", color: "#a074c4" },
  gif:        { icon: "\u{f1c5}", color: "#a074c4" },
  svg:        { icon: "\u{f1c5}", color: "#ffb13b" },
  ico:        { icon: "\u{f1c5}", color: "#cbcb41" },

  // Lock / Config files
  lock:       { icon: "\u{f023}", color: "#89898b" },  //

  // Misc
  env:        { icon: "\u{f462}", color: "#faf743" },  //
  gitignore:  { icon: "\u{e702}", color: "#f14e32" },  //
  log:        { icon: "\u{f15c}", color: "#89898b" },
  wasm:       { icon: "\u{e6a1}", color: "#654ff0" },  //
};

// Special filename matches (checked before extension)
const NAME_ICONS: Record<string, FileIcon> = {
  "Dockerfile":     { icon: "\u{e7b0}", color: "#384d54" },
  "Makefile":       { icon: "\u{e673}", color: "#6d8086" },
  "Cargo.toml":     { icon: "\u{e7a8}", color: "#dea584" },
  "Cargo.lock":     { icon: "\u{e7a8}", color: "#89898b" },
  "package.json":   { icon: "\u{e74e}", color: "#e8274b" },
  "tsconfig.json":  { icon: "\u{e628}", color: "#3178c6" },
  ".gitignore":     { icon: "\u{e702}", color: "#f14e32" },
  ".env":           { icon: "\u{f462}", color: "#faf743" },
};

const DEFAULT_ICON: FileIcon = { icon: "\u{f15c}", color: "#89898b" };  //
const EDIT_ICON: FileIcon = { icon: "\u{f040}", color: "#e2b93d" };     //

export function fileIcon(filePath: string): FileIcon {
  const name = filePath.replace(/\\/g, "/").split("/").pop() || "";

  // Check full filename first
  if (NAME_ICONS[name]) return NAME_ICONS[name];

  // Check extension
  const ext = name.includes(".") ? name.split(".").pop()?.toLowerCase() || "" : "";
  if (ext && EXT_ICONS[ext]) return EXT_ICONS[ext];

  return DEFAULT_ICON;
}

export function editIcon(): FileIcon {
  return EDIT_ICON;
}
