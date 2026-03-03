const EXT_TO_LANG: Record<string, string> = {
  ts: "typescript", tsx: "tsx", js: "javascript", jsx: "jsx",
  rs: "rust", py: "python", rb: "ruby", go: "go",
  java: "java", kt: "kotlin", swift: "swift", cs: "csharp",
  css: "css", scss: "scss", html: "html", vue: "vue", svelte: "svelte",
  json: "json", yaml: "yaml", yml: "yaml", toml: "toml",
  md: "markdown", sql: "sql", sh: "bash", zsh: "bash", bash: "bash",
  dockerfile: "dockerfile", graphql: "graphql",
};

export function langFromPath(filePath: string): string | undefined {
  const ext = filePath.split(".").pop()?.toLowerCase() || "";
  return EXT_TO_LANG[ext];
}
