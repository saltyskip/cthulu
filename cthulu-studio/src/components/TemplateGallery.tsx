/**
 * TemplateGallery — Vercel-style modal for picking a workflow template.
 *
 * Features per card:
 *  - Icon + title + description
 *  - Estimated cost badge
 *  - Category + tag pills
 *  - Live mini pipeline diagram (hover)
 *  - Inline YAML viewer (toggle)
 *  - GitHub raw link
 *  - "Use Template" one-click import
 */
import { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { listTemplates, importTemplate, importYaml, importFromGithub, deleteTemplate, getServerUrl } from "../api/client";
import type { TemplateMetadata, Flow } from "../types/flow";
import { Trash2 } from "lucide-react";
import MiniFlowDiagram from "./MiniFlowDiagram";

interface TemplateGalleryProps {
  onImport: (flow: Flow) => void;
  onBlank: () => void;
  onClose: () => void;
}

const GITHUB_RAW_BASE =
  "https://raw.githubusercontent.com/saltyskip/cthulu/main/static/workflows";

// Category display config
const CATEGORY_LABELS: Record<string, { label: string; emoji: string }> = {
  media: { label: "Media", emoji: "📰" },
  social: { label: "Social", emoji: "📱" },
  research: { label: "Research", emoji: "🔍" },
  finance: { label: "Finance", emoji: "📊" },
};

function getCategoryLabel(cat: string): string {
  return CATEGORY_LABELS[cat]?.label ?? cat.charAt(0).toUpperCase() + cat.slice(1);
}

function getCategoryEmoji(cat: string): string {
  return CATEGORY_LABELS[cat]?.emoji ?? "📁";
}

// Default icon per category if not set in meta
function defaultIcon(category: string): string {
  return getCategoryEmoji(category);
}

function YamlPanel({ slug, templates, onClose }: { slug: string; templates: TemplateMetadata[]; onClose: () => void }) {
  const tmpl = templates.find((t) => `${t.category}/${t.slug}` === slug);
  if (!tmpl) return null;
  return (
    <div className="tg-yaml-panel">
      <div className="tg-yaml-panel-header">
        <span className="tg-yaml-panel-title">{tmpl.title} — YAML</span>
        <button className="tg-yaml-panel-close" onClick={onClose}>✕</button>
      </div>
      <div className="tg-yaml-block">
        <pre className="tg-yaml-pre">{tmpl.raw_yaml}</pre>
      </div>
    </div>
  );
}

export default function TemplateGallery({
  onImport,
  onBlank,
  onClose,
}: TemplateGalleryProps) {
  const [templates, setTemplates] = useState<TemplateMetadata[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeCategory, setActiveCategory] = useState<string>("all");
  const [importingSlug, setImportingSlug] = useState<string | null>(null);
  const [expandedYaml, setExpandedYaml] = useState<string | null>(null);
  const [hoveredCard, setHoveredCard] = useState<string | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  // Upload / GitHub import state
  const [ghUrl, setGhUrl] = useState("");
  const [ghBranch, setGhBranch] = useState("main");
  const [ghPath, setGhPath] = useState("");
  const [ghWorking, setGhWorking] = useState(false);
  const [uploadWorking, setUploadWorking] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    listTemplates()
      .then((data) => {
        setTemplates(data);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  const categories = useMemo(() => {
    const cats = Array.from(new Set(templates.map((t) => t.category))).sort();
    return cats;
  }, [templates]);

  const filtered = useMemo(() => {
    let result = activeCategory === "all" ? templates : templates.filter((t) => t.category === activeCategory);
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      result = result.filter(
        (t) =>
          t.title.toLowerCase().includes(q) ||
          t.description.toLowerCase().includes(q) ||
          t.tags.some((tag) => tag.toLowerCase().includes(q)) ||
          t.slug.toLowerCase().includes(q)
      );
    }
    return result;
  }, [templates, activeCategory, searchQuery]);

  const handleImport = useCallback(
    async (template: TemplateMetadata) => {
      const key = `${template.category}/${template.slug}`;
      setImportingSlug(key);
      setImportError(null);
      try {
        const flow = await importTemplate(template.category, template.slug);
        onImport(flow);
      } catch (e) {
        setImportError(`Failed to import "${template.title}": ${(e as Error).message}`);
      } finally {
        setImportingSlug(null);
      }
    },
    [onImport]
  );

  const toggleYaml = useCallback(
    (key: string) => {
      setExpandedYaml((prev) => (prev === key ? null : key));
    },
    []
  );

  const handleDeleteTemplate = useCallback(
    async (tmpl: TemplateMetadata) => {
      if (!confirm(`Delete template "${tmpl.title}"? This removes the YAML file from disk.`)) return;
      setImportError(null);
      try {
        await deleteTemplate(tmpl.category, tmpl.slug);
        setTemplates((prev) => prev.filter((t) => !(t.category === tmpl.category && t.slug === tmpl.slug)));
      } catch (e) {
        setImportError(`Failed to delete "${tmpl.title}": ${(e as Error).message}`);
      }
    },
    []
  );

  // Upload YAML file(s)
  const handleFileUpload = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    if (files.length === 0) return;
    setUploadWorking(true);
    setImportError(null);
    let imported: Flow[] = [];
    let errs: string[] = [];
    for (const file of files) {
      try {
        const text = await file.text();
        const result = await importYaml(text);
        imported = imported.concat(result.flows);
        errs = errs.concat(result.errors.map((e) => `${file.name}: ${e.error}`));
      } catch (ex) {
        errs.push(`${file.name}: ${(ex as Error).message}`);
      }
    }
    setUploadWorking(false);
    if (fileInputRef.current) fileInputRef.current.value = "";
    if (errs.length > 0) setImportError(errs.join(" · "));
    if (imported.length > 0) {
      // Navigate to the last imported flow; if multiple were imported just open the first
      onImport(imported[0]);
    }
  }, [onImport]);

  // Import from GitHub repo
  const handleGithubImport = useCallback(async () => {
    if (!ghUrl.trim()) return;
    setGhWorking(true);
    setImportError(null);
    try {
      const result = await importFromGithub(ghUrl.trim(), ghPath.trim(), ghBranch.trim() || "main");
      if (result.errors.length > 0 && result.imported === 0) {
        setImportError(result.errors.map((e) => `${e.file}: ${e.error}`).join(" · "));
      } else {
        if (result.errors.length > 0) {
          setImportError(`Imported ${result.imported}/${result.total_found}. Errors: ` +
            result.errors.map((e) => e.file).join(", "));
        }
        if (result.flows.length > 0) onImport(result.flows[0]);
      }
    } catch (ex) {
      setImportError((ex as Error).message);
    } finally {
      setGhWorking(false);
    }
  }, [ghUrl, ghPath, ghBranch, onImport]);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  return (
    <div className="tg-overlay" onClick={onClose}>
      <div
        className="tg-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Choose a template"
      >
        {/* Header */}
        <div className="tg-header">
          <div>
            <h2 className="tg-title">Choose a Template</h2>
            <p className="tg-subtitle">
              Start with a pre-built workflow — all imported as disabled, ready to configure.
            </p>
          </div>
          <button className="tg-close" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>

        {/* Search bar */}
        <div className="tg-search-bar">
          <input
            className="tg-search-input"
            type="text"
            placeholder="Search templates by name, tag, or description…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            autoFocus
          />
          {searchQuery && (
            <button className="tg-search-clear" onClick={() => setSearchQuery("")}>
              ✕
            </button>
          )}
        </div>

        {/* Import panel: Upload YAML + GitHub */}
        <div className="tg-import-panel">
          {/* Upload */}
          <div className="tg-import-block">
            <div className="tg-import-label">Upload YAML</div>
            <div className="tg-import-row">
              <input
                ref={fileInputRef}
                type="file"
                accept=".yaml,.yml"
                multiple
                style={{ display: "none" }}
                onChange={handleFileUpload}
              />
              <button
                className="tg-import-btn"
                onClick={() => fileInputRef.current?.click()}
                disabled={uploadWorking}
              >
                {uploadWorking ? "Importing…" : "Choose file(s)…"}
              </button>
              <span className="tg-import-hint">.yaml / .yml — multiple OK</span>
            </div>
          </div>

          <div className="tg-import-divider" />

          {/* GitHub */}
          <div className="tg-import-block tg-import-block-gh">
            <div className="tg-import-label">GitHub Repo</div>
            <div className="tg-import-row">
              <input
                className="tg-import-input tg-import-input-url"
                type="text"
                placeholder="https://github.com/owner/repo"
                value={ghUrl}
                onChange={(e) => setGhUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleGithubImport()}
              />
              <input
                className="tg-import-input tg-import-input-sm"
                type="text"
                placeholder="branch"
                value={ghBranch}
                onChange={(e) => setGhBranch(e.target.value)}
                title="Branch (default: main)"
              />
              <input
                className="tg-import-input tg-import-input-sm"
                type="text"
                placeholder="path"
                value={ghPath}
                onChange={(e) => setGhPath(e.target.value)}
                title="Sub-path within repo (optional)"
              />
              <button
                className="tg-import-btn tg-import-btn-primary"
                onClick={handleGithubImport}
                disabled={ghWorking || !ghUrl.trim()}
              >
                {ghWorking ? "Importing…" : "Import"}
              </button>
            </div>
          </div>
        </div>

        {/* Category tabs */}
        <div className="tg-tabs">
          <button
            className={`tg-tab ${activeCategory === "all" ? "active" : ""}`}
            onClick={() => setActiveCategory("all")}
          >
            All
          </button>
          {categories.map((cat) => (
            <button
              key={cat}
              className={`tg-tab ${activeCategory === cat ? "active" : ""}`}
              onClick={() => setActiveCategory(cat)}
            >
              {getCategoryEmoji(cat)} {getCategoryLabel(cat)}
            </button>
          ))}
          <div className="tg-tabs-spacer" />
          {/* Blank flow always accessible */}
          <button className="tg-tab tg-tab-blank" onClick={onBlank}>
            Blank →
          </button>
        </div>

        {/* Error banner */}
        {importError && (
          <div className="tg-error-banner">
            {importError}
            <button onClick={() => setImportError(null)}>✕</button>
          </div>
        )}

        {/* Body */}
        <div className="tg-body">
          {loading && (
            <div className="tg-loading">
              <span className="tg-spinner" /> Loading templates…
            </div>
          )}

          {error && (
            <div className="tg-empty">
              <span style={{ color: "var(--text-secondary)" }}>
                Could not load templates: {error}
              </span>
            </div>
          )}

          {!loading && !error && filtered.length === 0 && (
            <div className="tg-empty">No templates in this category.</div>
          )}

          {!loading && !error && (
            <div className="tg-grid">
              {filtered.map((tmpl) => {
                const key = `${tmpl.category}/${tmpl.slug}`;
                const isImporting = importingSlug === key;
                const isYamlOpen = expandedYaml === key;
                const isHovered = hoveredCard === key;
                const icon = tmpl.icon ?? defaultIcon(tmpl.category);
                const ghUrl = `${GITHUB_RAW_BASE}/${tmpl.category}/${tmpl.slug}.yaml`;

                return (
                  <div
                    key={key}
                    className={`tg-card ${isHovered ? "hovered" : ""}${isYamlOpen ? " yaml-active" : ""}`}
                    onMouseEnter={() => setHoveredCard(key)}
                    onMouseLeave={() => setHoveredCard(null)}
                  >
                    {/* Card top: icon + title + meta row */}
                    <div className="tg-card-header">
                      <span className="tg-card-icon">{icon}</span>
                      <div className="tg-card-title-block">
                        <h3 className="tg-card-title">{tmpl.title}</h3>
                        <p className="tg-card-desc">{tmpl.description}</p>
                      </div>
                    </div>

                    {/* Mini diagram — shown on hover */}
                    <div className={`tg-mini-diagram ${isHovered ? "visible" : ""}`}>
                      {isHovered && (
                        <MiniFlowDiagram shape={tmpl.pipeline_shape} />
                      )}
                    </div>

                    {/* Tags + cost */}
                    <div className="tg-card-meta">
                      <div className="tg-tags">
                        <span className="tg-tag tg-tag-category">
                          {getCategoryLabel(tmpl.category)}
                        </span>
                        {tmpl.tags.slice(0, 3).map((tag) => (
                          <span key={tag} className="tg-tag">
                            {tag}
                          </span>
                        ))}
                      </div>
                      {tmpl.estimated_cost && (
                        <span className="tg-cost-badge">
                          {tmpl.estimated_cost}
                        </span>
                      )}
                    </div>

                    {/* Footer actions */}
                    <div className="tg-card-footer">
                      <button
                        className="tg-btn-yaml"
                        onClick={() => toggleYaml(key)}
                        title="View YAML"
                      >
                        {isYamlOpen ? "YAML ▴" : "YAML ▾"}
                      </button>
                      <a
                        className="tg-btn-gh"
                        href={ghUrl}
                        target="_blank"
                        rel="noopener noreferrer"
                        title="View on GitHub"
                        onClick={(e) => e.stopPropagation()}
                      >
                        ↗
                      </a>
                      <button
                        className="tg-btn-delete"
                        onClick={() => handleDeleteTemplate(tmpl)}
                        title="Delete template"
                      >
                        <Trash2 size={13} />
                      </button>
                      <button
                        className="tg-btn-use"
                        onClick={() => handleImport(tmpl)}
                        disabled={isImporting}
                      >
                        {isImporting ? "Importing…" : "Use Template"}
                      </button>
                    </div>
                  </div>
                );
              })}

              {/* Blank card — always last in "all" view */}
              {activeCategory === "all" && (
                <div className="tg-card tg-card-blank" onClick={onBlank}>
                  <div className="tg-card-header">
                    <span className="tg-card-icon">✦</span>
                    <div className="tg-card-title-block">
                      <h3 className="tg-card-title">Start from Scratch</h3>
                      <p className="tg-card-desc">
                        Build your own workflow with an empty canvas.
                      </p>
                    </div>
                  </div>
                  <div className="tg-card-footer" style={{ marginTop: "auto" }}>
                    <button className="tg-btn-use tg-btn-blank">
                      Create Blank →
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* YAML preview panel — shown below the grid for the selected template */}
          {expandedYaml && <YamlPanel slug={expandedYaml} templates={templates} onClose={() => setExpandedYaml(null)} />}
        </div>

        {/* Footer hint */}
        <div className="tg-footer-hint">
          Add templates by dropping <code>.yaml</code> files into{" "}
          <code>static/workflows/&#123;category&#125;/</code>
        </div>
      </div>
    </div>
  );
}
