import { useState } from "react";
import type { FlowSummary, Flow, ActiveView } from "../types/flow";
import { Switch } from "@/components/ui/switch";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import TemplateGallery from "./TemplateGallery";

interface FlowListProps {
  flows: FlowSummary[];
  activeFlowId: string | null;
  onSelectFlow: (id: string) => void;
  onCreateFlow: () => void;
  onImportTemplate: (flow: Flow) => void;
  onToggleEnabled: (flowId: string) => void;
  activeView: ActiveView;
}

export default function FlowList({
  flows,
  activeFlowId,
  onSelectFlow,
  onCreateFlow,
  onImportTemplate,
  onToggleEnabled,
  activeView,
}: FlowListProps) {
  const [showGallery, setShowGallery] = useState(false);

  function handleNewFlowClick() {
    setShowGallery(true);
  }

  function handleGalleryImport(flow: Flow) {
    setShowGallery(false);
    onImportTemplate(flow);
  }

  function handleBlank() {
    setShowGallery(false);
    onCreateFlow();
  }

  return (
    <>
      {showGallery && (
        <TemplateGallery
          onImport={handleGalleryImport}
          onBlank={handleBlank}
          onClose={() => setShowGallery(false)}
        />
      )}

      <Collapsible className="sidebar-section">
        <CollapsibleTrigger asChild>
          <div className="sidebar-section-header">
            <span className="sidebar-chevron">▶</span>
            <h2>Flows</h2>
            <div style={{ flex: 1 }} />
            <button
              className="ghost sidebar-action-btn"
              onClick={(e) => {
                e.stopPropagation();
                handleNewFlowClick();
              }}
            >
              +
            </button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="sidebar-section-body">
            {flows.map((flow) => (
              <div
                key={flow.id}
                className={`sidebar-item${flow.id === activeFlowId && activeView === "flow-editor" ? " active" : ""}${!flow.enabled ? " disabled" : ""}`}
                onClick={() => onSelectFlow(flow.id)}
              >
                <div className="sidebar-item-row">
                  <div className="sidebar-item-name">{flow.name}</div>
                  <Switch
                    checked={flow.enabled}
                    onCheckedChange={() => onToggleEnabled(flow.id)}
                    onClick={(e) => e.stopPropagation()}
                    className="data-[state=checked]:bg-[var(--success)]"
                  />
                </div>
                <div className="sidebar-item-meta">{flow.node_count} nodes</div>
              </div>
            ))}
            {flows.length === 0 && (
              <div className="sidebar-item-empty">No flows yet</div>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </>
  );
}
