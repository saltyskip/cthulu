import { useState, useEffect, useCallback, useMemo } from "react";
import * as api from "../api/client";
import type { Agent, AgentSummary, ProjectMeta } from "../types/flow";
import { STUDIO_ASSISTANT_ID, AGENT_ROLES, ROLE_LABELS } from "../types/flow";
import type { AgentRole } from "../types/flow";
import { useOrg } from "../contexts/OrgContext";

interface AgentConfigPageProps {
  agent: Agent;
  onAgentUpdated: () => void;
}

export function AgentConfigPage({ agent, onAgentUpdated }: AgentConfigPageProps) {
  const [name, setName] = useState(agent.name);
  const [description, setDescription] = useState(agent.description);
  const [prompt, setPrompt] = useState(agent.prompt);
  const [permissions, setPermissions] = useState(agent.permissions.join(", "));
  const [systemPrompt, setSystemPrompt] = useState(agent.append_system_prompt ?? "");
  const [workingDir, setWorkingDir] = useState(agent.working_dir ?? "");
  const [heartbeatEnabled, setHeartbeatEnabled] = useState(agent.heartbeat_enabled);
  const [heartbeatInterval, setHeartbeatInterval] = useState(agent.heartbeat_interval_secs);
  const [heartbeatPrompt, setHeartbeatPrompt] = useState(agent.heartbeat_prompt_template);
  const [maxTurns, setMaxTurns] = useState(agent.max_turns_per_heartbeat);
  const [autoPerms, setAutoPerms] = useState(agent.auto_permissions);
  const [role, setRole] = useState<string>(agent.role ?? "");
  const [reportsTo, setReportsTo] = useState<string>(agent.reports_to ?? "");
  const [allAgents, setAllAgents] = useState<AgentSummary[]>([]);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  // Reset form when agent changes
  useEffect(() => {
    setName(agent.name);
    setDescription(agent.description);
    setPrompt(agent.prompt);
    setPermissions(agent.permissions.join(", "));
    setSystemPrompt(agent.append_system_prompt ?? "");
    setWorkingDir(agent.working_dir ?? "");
    setHeartbeatEnabled(agent.heartbeat_enabled);
    setHeartbeatInterval(agent.heartbeat_interval_secs);
    setHeartbeatPrompt(agent.heartbeat_prompt_template);
    setMaxTurns(agent.max_turns_per_heartbeat);
    setAutoPerms(agent.auto_permissions);
    setRole(agent.role ?? "");
    setReportsTo(agent.reports_to ?? "");
    setDirty(false);
  }, [agent]);

  const markDirty = useCallback(() => setDirty(true), []);

  // Load agents list for "Reports To" dropdown
  useEffect(() => {
    api.listAgents().then(setAllAgents).catch(() => setAllAgents([]));
  }, []);

  const reportsToOptions = useMemo(() => {
    return allAgents.filter(
      (a) => a.id !== agent.id && a.id !== STUDIO_ASSISTANT_ID && !a.subagent_only
    );
  }, [allAgents, agent.id]);

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      await api.updateAgent(agent.id, {
        name,
        description,
        prompt,
        permissions: permissions.split(",").map(p => p.trim()).filter(Boolean),
        append_system_prompt: systemPrompt || null,
        working_dir: workingDir || null,
        heartbeat_enabled: heartbeatEnabled,
        heartbeat_interval_secs: heartbeatInterval,
        heartbeat_prompt_template: heartbeatPrompt,
        max_turns_per_heartbeat: maxTurns,
        auto_permissions: autoPerms,
        role: role || null,
        reports_to: reportsTo || null,
      });
      setDirty(false);
      onAgentUpdated();
    } catch (e) {
      console.error("Failed to save agent config:", e);
    } finally {
      setSaving(false);
    }
  }, [agent.id, name, description, prompt, permissions, systemPrompt, workingDir,
      heartbeatEnabled, heartbeatInterval, heartbeatPrompt, maxTurns, autoPerms,
      role, reportsTo, onAgentUpdated]);

  const handleCancel = useCallback(() => {
    setName(agent.name);
    setDescription(agent.description);
    setPrompt(agent.prompt);
    setPermissions(agent.permissions.join(", "));
    setSystemPrompt(agent.append_system_prompt ?? "");
    setWorkingDir(agent.working_dir ?? "");
    setHeartbeatEnabled(agent.heartbeat_enabled);
    setHeartbeatInterval(agent.heartbeat_interval_secs);
    setHeartbeatPrompt(agent.heartbeat_prompt_template);
    setMaxTurns(agent.max_turns_per_heartbeat);
    setAutoPerms(agent.auto_permissions);
    setRole(agent.role ?? "");
    setReportsTo(agent.reports_to ?? "");
    setDirty(false);
  }, [agent]);

  const isStudioAssistant = agent.id === STUDIO_ASSISTANT_ID;

  // --- Project assignment (publish to org) ---
  const { selectedOrgSlug, selectedOrg } = useOrg();
  const [orgProjects, setOrgProjects] = useState<ProjectMeta[]>([]);
  const [selectedProject, setSelectedProject] = useState<string>(agent.project ?? "");
  const [publishing, setPublishing] = useState(false);
  const [publishError, setPublishError] = useState<string | null>(null);

  useEffect(() => {
    if (!selectedOrgSlug) return;
    api.listAgentProjects(selectedOrgSlug).then(setOrgProjects).catch(() => setOrgProjects([]));
  }, [selectedOrgSlug]);

  useEffect(() => {
    setSelectedProject(agent.project ?? "");
  }, [agent.project]);

  const isPublished = useMemo(() => {
    return !!agent.project && orgProjects.some(p => p.slug === agent.project);
  }, [agent.project, orgProjects]);

  const handlePublish = useCallback(async () => {
    if (!selectedOrgSlug || !selectedProject) return;
    setPublishing(true);
    setPublishError(null);
    try {
      await api.publishAgent(agent.id, selectedOrgSlug, selectedProject);
      onAgentUpdated();
    } catch (e) {
      setPublishError(typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
    } finally {
      setPublishing(false);
    }
  }, [agent.id, selectedOrgSlug, selectedProject, onAgentUpdated]);

  const handleUnpublish = useCallback(async () => {
    if (!selectedOrgSlug) return;
    setPublishing(true);
    setPublishError(null);
    try {
      await api.unpublishAgent(agent.id, selectedOrgSlug);
      onAgentUpdated();
    } catch (e) {
      setPublishError(typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
    } finally {
      setPublishing(false);
    }
  }, [agent.id, selectedOrgSlug, onAgentUpdated]);

  return (
    <div className="agent-config-page">
      {/* Floating Save Bar */}
      {dirty && (
        <div className="config-save-bar">
          <span>Unsaved changes</span>
          <div className="config-save-bar-actions">
            <button className="config-cancel-btn" onClick={handleCancel}>Cancel</button>
            <button className="config-save-btn" onClick={handleSave} disabled={saving}>
              {saving ? "Saving..." : "Save"}
            </button>
          </div>
        </div>
      )}

      {/* Identity Card */}
      <div className="config-card">
        <h3 className="config-card-title">Identity</h3>
        <div className="config-card-fields">
          <div className="config-field">
            <label>Name</label>
            <input value={name} onChange={e => { setName(e.target.value); markDirty(); }} />
          </div>
          <div className="config-field">
            <label>Description</label>
            <input value={description} onChange={e => { setDescription(e.target.value); markDirty(); }} placeholder="What does this agent do?" />
          </div>
          <div className="config-field">
            <label>Prompt</label>
            <textarea rows={6} value={prompt} onChange={e => { setPrompt(e.target.value); markDirty(); }} placeholder="Prompt text or file path" />
          </div>
          {!agent.project && (
            <div className="config-field">
              <label>Permissions</label>
              <input value={permissions} onChange={e => { setPermissions(e.target.value); markDirty(); }} placeholder="Bash, Read, Grep, Glob" />
            </div>
          )}
          <div className="config-field">
            <label>System Prompt</label>
            <textarea rows={4} value={systemPrompt} onChange={e => { setSystemPrompt(e.target.value); markDirty(); }} placeholder="Additional instructions" />
          </div>
          {!agent.project ? (
            <div className="config-field">
              <label>Working Directory</label>
              <input value={workingDir} onChange={e => { setWorkingDir(e.target.value); markDirty(); }} placeholder="/path/to/directory" />
            </div>
          ) : (
            <div className="config-field">
              <label>Working Directory</label>
              <input value={agent.working_dir ?? ""} disabled className="config-field-readonly" />
              <p className="config-card-hint">Inherited from project. Change it in project settings.</p>
            </div>
          )}
        </div>
      </div>

      {/* Hierarchy Card */}
      <div className="config-card">
        <h3 className="config-card-title">Hierarchy</h3>
        <div className="config-card-fields">
          <div className="config-field">
            <label>Role</label>
            <select value={role} onChange={e => { setRole(e.target.value); markDirty(); }}>
              <option value="">None</option>
              {AGENT_ROLES.map((r) => (
                <option key={r} value={r}>{ROLE_LABELS[r]}</option>
              ))}
            </select>
          </div>
          <div className="config-field">
            <label>Reports To</label>
            <select value={reportsTo} onChange={e => { setReportsTo(e.target.value); markDirty(); }}>
              <option value="">None</option>
              {reportsToOptions.map((a) => (
                <option key={a.id} value={a.id}>{a.name}</option>
              ))}
            </select>
          </div>
        </div>
      </div>

      {/* Heartbeat Card */}
      <div className="config-card">
        <h3 className="config-card-title">Heartbeat</h3>
        <div className="config-card-fields">
          <div className="config-field config-field-row">
            <label>Enable Heartbeat</label>
            <input type="checkbox" checked={heartbeatEnabled} onChange={e => { setHeartbeatEnabled(e.target.checked); markDirty(); }} />
          </div>
          {heartbeatEnabled && (
            <>
              <div className="config-field">
                <label>Interval (seconds)</label>
                <input type="number" min={10} value={heartbeatInterval} onChange={e => { setHeartbeatInterval(Number(e.target.value)); markDirty(); }} />
              </div>
              <div className="config-field">
                <label>Heartbeat Prompt</label>
                <textarea rows={4} value={heartbeatPrompt} onChange={e => { setHeartbeatPrompt(e.target.value); markDirty(); }} />
              </div>
              <div className="config-field">
                <label>Max Turns per Heartbeat</label>
                <input type="number" min={1} value={maxTurns} onChange={e => { setMaxTurns(Number(e.target.value)); markDirty(); }} />
              </div>
              <div className="config-field config-field-row">
                <label>Auto-approve permissions</label>
                <input type="checkbox" checked={autoPerms} onChange={e => { setAutoPerms(e.target.checked); markDirty(); }} />
              </div>
            </>
          )}
        </div>
      </div>

      {/* Project / Publish Card */}
      {!isStudioAssistant && selectedOrgSlug && (
        <div className="config-card">
          <h3 className="config-card-title">Project</h3>
          <p className="config-card-hint">
            Assign this agent to a project in <strong>{selectedOrg?.name ?? selectedOrgSlug}</strong> to sync it to your <code>cthulu-agents</code> repo.
          </p>
          <div className="config-card-fields">
            <div className="config-field">
              <label>Project</label>
              <select
                value={selectedProject}
                onChange={e => setSelectedProject(e.target.value)}
                disabled={publishing}
              >
                <option value="">— None —</option>
                {orgProjects.map(p => (
                  <option key={p.slug} value={p.slug}>{p.name}</option>
                ))}
              </select>
            </div>
            {publishError && (
              <p className="config-publish-error">{publishError}</p>
            )}
            <div className="config-publish-actions">
              {isPublished ? (
                <>
                  <span className="config-publish-status">
                    Published to <strong>{agent.project}</strong>
                  </span>
                  <button
                    className="config-unpublish-btn"
                    onClick={handleUnpublish}
                    disabled={publishing}
                  >
                    {publishing ? "Removing..." : "Unpublish"}
                  </button>
                  {selectedProject !== agent.project && selectedProject && (
                    <button
                      className="config-save-btn"
                      onClick={handlePublish}
                      disabled={publishing}
                    >
                      {publishing ? "Moving..." : `Move to ${selectedProject}`}
                    </button>
                  )}
                </>
              ) : (
                <button
                  className="config-save-btn"
                  onClick={handlePublish}
                  disabled={publishing || !selectedProject}
                >
                  {publishing ? "Publishing..." : "Publish"}
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Footer */}
      {!isStudioAssistant && (
        <div className="config-footer">
          <span className="config-agent-id">ID: {agent.id}</span>
        </div>
      )}
    </div>
  );
}
