import { useState, useEffect, useMemo, useCallback } from "react";
import * as api from "../api/client";
import type { AgentSummary, ProjectMeta } from "../types/flow";
import { useOrg } from "../contexts/OrgContext";
import { ChevronRight, Plus, FolderOpen } from "lucide-react";
import { STUDIO_ASSISTANT_ID } from "../types/flow";
import { NewProjectDialog } from "./NewProjectDialog";

interface SidebarProjectsProps {
  agents: AgentSummary[];
  onSelectAgent: (agentId: string) => void;
  selectedAgentId: string | null;
}

export function SidebarProjects({ agents, onSelectAgent, selectedAgentId }: SidebarProjectsProps) {
  const { selectedOrgSlug } = useOrg();
  const [projects, setProjects] = useState<ProjectMeta[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [showNewProject, setShowNewProject] = useState(false);

  // Load projects when org changes
  useEffect(() => {
    if (!selectedOrgSlug) {
      setProjects([]);
      return;
    }
    api.listAgentProjects(selectedOrgSlug).then(setProjects).catch(() => setProjects([]));
  }, [selectedOrgSlug]);

  // Group agents by project
  const agentsByProject = useMemo(() => {
    const map = new Map<string, AgentSummary[]>();
    for (const project of projects) {
      map.set(project.slug, []);
    }
    for (const agent of agents) {
      if (agent.id === STUDIO_ASSISTANT_ID || agent.subagent_only) continue;
      if (agent.project && map.has(agent.project)) {
        map.get(agent.project)!.push(agent);
      }
    }
    return map;
  }, [agents, projects]);

  const toggleProject = useCallback((projectSlug: string) => {
    setExpanded(prev => {
      const next = new Set(prev);
      if (next.has(projectSlug)) next.delete(projectSlug);
      else next.add(projectSlug);
      return next;
    });
  }, []);

  const handleProjectCreated = useCallback(() => {
    setShowNewProject(false);
    if (selectedOrgSlug) {
      api.listAgentProjects(selectedOrgSlug).then(setProjects).catch(() => {});
    }
  }, [selectedOrgSlug]);

  if (!selectedOrgSlug) return null;

  return (
    <div className="sb-projects-section">
      {projects.map(project => {
        const isExpanded = expanded.has(project.slug);
        const projectAgents = agentsByProject.get(project.slug) ?? [];
        return (
          <div key={project.slug} className="sb-project-group">
            <button className="sb-project-header" onClick={() => toggleProject(project.slug)}>
              <ChevronRight
                size={12}
                className={`sb-project-chevron${isExpanded ? " sb-project-chevron-open" : ""}`}
              />
              <FolderOpen size={14} className="sb-project-icon" />
              <span className="sb-project-name">{project.name}</span>
              <span className="sb-project-count">{projectAgents.length}</span>
            </button>
            {isExpanded && (
              <div className="sb-project-agents">
                {projectAgents.length === 0 ? (
                  <div className="sb-project-empty">No agents</div>
                ) : (
                  projectAgents.map(agent => (
                    <button
                      key={agent.id}
                      className={`sb-agent-item sb-project-agent-item${selectedAgentId === agent.id ? " sb-agent-item-active" : ""}`}
                      onClick={() => onSelectAgent(agent.id)}
                    >
                      <span className="sb-agent-name">{agent.name}</span>
                    </button>
                  ))
                )}
              </div>
            )}
          </div>
        );
      })}
      <button className="sb-project-add-btn" onClick={() => setShowNewProject(true)}>
        <Plus size={12} />
        New Project
      </button>
      {showNewProject && (
        <NewProjectDialog
          onClose={() => setShowNewProject(false)}
          onCreated={handleProjectCreated}
        />
      )}
    </div>
  );
}
