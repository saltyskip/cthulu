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
import { useState, useEffect, useRef, useMemo, useCallback, useDeferredValue } from "react";
import { listTemplates, importTemplate, importYaml, importFromGithub, getServerUrl } from "../api/client";
import type { TemplateMetadata, Flow } from "../types/flow";
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
  const deferredSearch = useDeferredValue(searchQuery);
  const searchRef = useRef<HTMLInputElement>(null);

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

  /**
   * Consolidated filtering: category first, then text search.
   * Uses deferredSearch so the input stays snappy even with 500+ templates.
   */
  const filtered = useMemo(() => {
    // Step 1: filter by category
    const byCategory =
      activeCategory === "all"
        ? templates
        : templates.filter((t) => t.category === activeCategory);

    // Step 2: filter by search query
    const q = deferredSearch.trim().toLowerCase();
    if (!q) return byCategory;

    return byCategory.filter((t) =>
      t.title.toLowerCase().includes(q) ||
      t.description.toLowerCase().includes(q) ||
      t.slug.toLowerCase().includes(q) ||
      t.category.toLowerCase().includes(q) ||
      t.tags.some((tag) => tag.toLowerCase().includes(q))
    );
  }, [templates, activeCategory, deferredSearch]);

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

  /**
   * Keyboard UX + auto-focus (single consolidated effect).
   * - Auto-focuses the search input on mount.
   * - Escape: clears search if input is focused and has text, otherwise closes modal.
   */
  useEffect(() => {
    const focusTimer = setTimeout(() => searchRef.current?.focus(), 100);

    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (searchQuery && document.activeElement === searchRef.current) {
          setSearchQuery("");
        } else {
          onClose();
        }
      }
    };
    window.addEventListener("keydown", handler);

    return () => {
      clearTimeout(focusTimer);
      window.removeEventListener("keydown", handler);
    };
  }, [onClose, searchQuery]);

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
        <div className="tg-search">
          <svg
            className="tg-search-icon"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            ref={searchRef}
            className="tg-search-input"
            type="text"
            placeholder="Search templates by name, tag, or category..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          {searchQuery && (
            <button
              className="tg-search-clear"
              onClick={() => { setSearchQuery(""); searchRef.current?.focus(); }}
              aria-label="Clear search"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
                <line x1="6" y1="6" x2="18" y2="18" />
                <line x1="6" y1="18" x2="18" y2="6" />
              </svg>
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
            <div className="tg-empty">
              {deferredSearch
                ? `No templates matching "${deferredSearch}".`
                : "No templates in this category."}
            </div>
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
                    className={`tg-card ${isHovered ? "hovered" : ""}`}
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

                    {/* YAML inline viewer */}
                    {isYamlOpen && (
                      <div className="tg-yaml-block">
                        <pre className="tg-yaml-pre">{tmpl.raw_yaml}</pre>
                      </div>
                    )}

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
