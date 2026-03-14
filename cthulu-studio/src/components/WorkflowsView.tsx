import { useState, useEffect, useCallback, useRef, useMemo, useDeferredValue } from "react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import * as api from "../api/client";
import type { WorkflowSummary, TemplateMetadata } from "../types/flow";
import GitHubPatDialog from "./GitHubPatDialog";
import CreateWorkflowDialog from "./CreateWorkflowDialog";
import TemplateGallery from "./TemplateGallery";
import { useWorkflowContext } from "../contexts/WorkflowContext";
import { Search, X } from "lucide-react";

interface WorkflowsViewProps {
  onOpenWorkflow: (workspace: string, name: string) => void;
  onWorkflowsChanged?: (workspaces: string[], workflows: WorkflowSummary[]) => void;
  /** Increment to trigger the template gallery from outside (e.g. sidebar "+" button) */
  newWorkflowTrigger?: number;
  /** Controlled active workspace — owned by parent (App.tsx) */
  activeWorkspace?: string | null;
  /** Notify parent when workspace should change */
  onSelectWorkspace?: (ws: string) => void;
}

export default function WorkflowsView({ onOpenWorkflow, onWorkflowsChanged, newWorkflowTrigger, activeWorkspace, onSelectWorkspace }: WorkflowsViewProps) {
  const [patConfigured, setPatConfigured] = useState<boolean | null>(null);
  const [showPatDialog, setShowPatDialog] = useState(false);
  const [showNewWorkflow, setShowNewWorkflow] = useState(false);
  const [repoReady, setRepoReady] = useState(false);
  const [setting, setSetting] = useState(false);

  const [workspaces, setWorkspaces] = useState<string[]>([]);
  const [workflows, setWorkflows] = useState<WorkflowSummary[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showTemplateGallery, setShowTemplateGallery] = useState(false);
  const [selectedTemplate, setSelectedTemplate] = useState<TemplateMetadata | null>(null);
  const [cachedTemplates, setCachedTemplates] = useState<TemplateMetadata[] | undefined>(undefined);

  // Shared state from WorkflowContext
  const { toggleWorkflowEnabled, isWorkflowEnabled, workflowSearch, setWorkflowSearch } = useWorkflowContext();
  const deferredSearch = useDeferredValue(workflowSearch);
  const searchRef = useRef<HTMLInputElement>(null);

  const filteredWorkflows = useMemo(() => {
    const q = deferredSearch.trim().toLowerCase();
    if (!q) return workflows;
    return workflows.filter((wf) =>
      wf.name.toLowerCase().includes(q) ||
      (wf.description && wf.description.toLowerCase().includes(q))
    );
  }, [workflows, deferredSearch]);

  const handleToggleEnabled = (e: React.MouseEvent, ws: string, name: string) => {
    e.stopPropagation();
    toggleWorkflowEnabled(ws, name);
  };

  const handleRun = async (e: React.MouseEvent, ws: string, name: string) => {
    e.stopPropagation();
    try {
      await api.runWorkflow(ws, name);
    } catch (err) {
      console.error(`[WorkflowsView] Run workflow failed: ${ws}/${name}`, err);
    }
  };

  useEffect(() => {
    api.getGithubPatStatus()
      .then((res) => setPatConfigured(res.configured))
      .catch(() => setPatConfigured(false));
  }, []);

  const setupRepo = useCallback(async () => {
    setSetting(true);
    setError(null);
    try {
      await api.setupWorkflows();
      setRepoReady(true);
      const res = await api.listWorkspaces();
      setWorkspaces(res.workspaces);
      if (res.workspaces.length > 0) {
        onSelectWorkspace?.(res.workspaces[0]);
      }
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
    } finally {
      setSetting(false);
    }
  }, []);

  const handlePatSaved = useCallback(() => {
    setPatConfigured(true);
    setupRepo();
  }, [setupRepo]);

  useEffect(() => {
    if (patConfigured === true && !repoReady) {
      setupRepo();
    }
  }, [patConfigured, repoReady, setupRepo]);

  const refreshWorkflows = useCallback(async (ws: string | null) => {
    if (!ws || !repoReady) { setWorkflows([]); return; }
    try {
      const res = await api.listWorkflows(ws);
      setWorkflows(res.workflows);
    } catch { setWorkflows([]); }
  }, [repoReady]);

  useEffect(() => {
    refreshWorkflows(activeWorkspace ?? null);
  }, [activeWorkspace, repoReady, refreshWorkflows]);

  // Eagerly fetch templates once when repo is ready (cached for TemplateGallery)
  useEffect(() => {
    if (repoReady && !cachedTemplates) {
      api.listTemplates().then(setCachedTemplates).catch(() => {});
    }
  }, [repoReady, cachedTemplates]);

  // Sync state to parent so sidebar stays up to date
  // Skip initial render (workspaces=[]) to avoid clobbering eagerly loaded data
  const hasMountedRef = useRef(false);
  useEffect(() => {
    if (!hasMountedRef.current) {
      hasMountedRef.current = true;
      if (workspaces.length === 0) return; // skip initial empty sync
    }
    onWorkflowsChanged?.(workspaces, workflows);
  }, [workspaces, workflows, onWorkflowsChanged]);

  // External trigger to open the template gallery (e.g. sidebar "+" button)
  const prevTriggerRef = useRef(newWorkflowTrigger);
  useEffect(() => {
    if (newWorkflowTrigger && newWorkflowTrigger !== prevTriggerRef.current) {
      prevTriggerRef.current = newWorkflowTrigger;
      setShowTemplateGallery(true);
    }
  }, [newWorkflowTrigger]);

  const handleSync = async () => {
    setSyncing(true);
    try {
      const res = await api.syncWorkflows();
      setWorkspaces(res.workspaces);
      if (activeWorkspace) {
        await refreshWorkflows(activeWorkspace);
      }
    } catch {
      // logged
    } finally {
      setSyncing(false);
    }
  };

  const handleWorkflowCreated = async (workspace: string, name: string) => {
    // Optimistically add the new workflow to the list immediately
    setWorkflows((prev) => {
      if (prev.some((wf) => wf.name === name && wf.workspace === workspace)) return prev;
      return [...prev, { name, workspace, description: "", node_count: 0 }];
    });
    // Then refresh from backend to get accurate data
    await refreshWorkflows(workspace);
  };

  if (patConfigured === null) {
    return (
      <div className="workflows-view">
        <div className="workflows-empty">Checking configuration...</div>
      </div>
    );
  }

  if (!patConfigured) {
    return (
      <div className="workflows-view">
        <div className="workflows-empty">
          <p>Connect your GitHub account to store and sync workflows.</p>
          <Button onClick={() => setShowPatDialog(true)} className="mt-4">
            Connect GitHub
          </Button>
        </div>
        <GitHubPatDialog
          open={showPatDialog}
          onOpenChange={setShowPatDialog}
          onSaved={handlePatSaved}
        />
      </div>
    );
  }

  if (setting) {
    return (
      <div className="workflows-view">
        <div className="workflows-empty">Setting up workflows repository...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="workflows-view">
        <div className="workflows-empty">
          <p className="text-[var(--danger)]">{error}</p>
          <Button onClick={setupRepo} className="mt-4">Retry</Button>
        </div>
      </div>
    );
  }

  return (
    <div className="workflows-view">
      <div className="workflows-toolbar">
        <div className="wf-search">
          <Search size={14} className="wf-search-icon" />
          <input
            ref={searchRef}
            className="wf-search-input"
            type="text"
            placeholder="Search workflows..."
            value={workflowSearch}
            onChange={(e) => setWorkflowSearch(e.target.value)}
          />
          {workflowSearch && (
            <button
              className="wf-search-clear"
              onClick={() => { setWorkflowSearch(""); searchRef.current?.focus(); }}
              aria-label="Clear search"
            >
              <X size={12} />
            </button>
          )}
        </div>
        <div className="spacer" />
        <Button
          variant="ghost"
          size="sm"
          onClick={handleSync}
          disabled={syncing}
        >
          {syncing ? "Syncing..." : "Sync"}
        </Button>
      </div>

      <div className="workflow-grid">
        {filteredWorkflows.map((wf) => {
          const isEnabled = isWorkflowEnabled(wf.workspace, wf.name);
          return (
            <div
              key={wf.name}
              className={`workflow-card${isEnabled ? " workflow-card-active" : ""}`}
              onClick={() => onOpenWorkflow(wf.workspace, wf.name)}
            >
              <div className="workflow-card-controls">
                <Switch
                  checked={isEnabled}
                  onClick={(e) => handleToggleEnabled(e, wf.workspace, wf.name)}
                  className="data-[state=checked]:bg-[var(--success)]"
                />
                <Button
                  size="xs"
                  variant={isEnabled ? "default" : "ghost"}
                  onClick={(e) => handleRun(e, wf.workspace, wf.name)}
                  title={!isEnabled ? "Workflow is inactive — manual run" : "Run workflow"}
                >
                  {isEnabled ? "Run" : "Run (Manual)"}
                </Button>
              </div>
              <div className="workflow-card-name">{wf.name}</div>
              {wf.description && (
                <div className="workflow-card-desc">{wf.description}</div>
              )}
              <div className="workflow-card-meta">
                <span>{wf.node_count} node{wf.node_count !== 1 ? "s" : ""}</span>
                {isEnabled && <span className="workflow-active-badge">Active</span>}
              </div>
            </div>
          );
        })}
        {filteredWorkflows.length === 0 && deferredSearch && (
          <div className="workflow-card workflow-card-empty">
            <div className="workflow-card-name">No workflows matching "{deferredSearch}"</div>
          </div>
        )}
        {activeWorkspace && !deferredSearch && (
          <div
            className="workflow-card workflow-card-new"
            onClick={() => setShowTemplateGallery(true)}
          >
            <div className="workflow-card-name">+ New Workflow</div>
          </div>
        )}
      </div>

      {showTemplateGallery && (
        <TemplateGallery
          cachedTemplates={cachedTemplates}
          onSelectTemplate={(tmpl) => {
            setSelectedTemplate(tmpl);
            setShowTemplateGallery(false);
            setShowNewWorkflow(true);
          }}
          onBlank={() => {
            setSelectedTemplate(null);
            setShowTemplateGallery(false);
            setShowNewWorkflow(true);
          }}
          onClose={() => setShowTemplateGallery(false)}
          onImport={() => {/* unused in workflow mode */}}
        />
      )}

      {activeWorkspace && (
        <CreateWorkflowDialog
          open={showNewWorkflow}
          onOpenChange={setShowNewWorkflow}
          workspace={activeWorkspace}
          onCreated={handleWorkflowCreated}
          template={selectedTemplate}
        />
      )}
    </div>
  );
}
