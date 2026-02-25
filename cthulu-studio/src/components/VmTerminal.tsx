import { useState, useEffect, useCallback } from "react";
import * as api from "../api/client";

interface VmTerminalProps {
  flowId: string;
  nodeId: string;
  nodeLabel: string;
}

type VmState =
  | { status: "loading" }
  | { status: "creating" }
  | { status: "ready"; vm: api.VmInfo }
  | { status: "error"; message: string }
  | { status: "destroyed" };

export default function VmTerminal({
  flowId,
  nodeLabel,
}: VmTerminalProps) {
  const [vmState, setVmState] = useState<VmState>({ status: "loading" });

  // On mount, check if a VM already exists for this flow; if not, create one.
  useEffect(() => {
    let cancelled = false;

    async function init() {
      try {
        setVmState({ status: "loading" });

        // Check for existing VM
        const existing = await api.getFlowVm(flowId);
        if (cancelled) return;

        if (existing) {
          setVmState({ status: "ready", vm: existing });
          return;
        }

        // No VM exists — create one
        setVmState({ status: "creating" });
        const vm = await api.createFlowVm(flowId);
        if (cancelled) return;
        setVmState({ status: "ready", vm });
      } catch (e) {
        if (cancelled) return;
        setVmState({
          status: "error",
          message: (e as Error).message || "Failed to initialize VM",
        });
      }
    }

    init();
    return () => {
      cancelled = true;
    };
  }, [flowId]);

  const handleDestroy = useCallback(async () => {
    try {
      await api.deleteFlowVm(flowId);
      setVmState({ status: "destroyed" });
    } catch (e) {
      setVmState({
        status: "error",
        message: (e as Error).message || "Failed to destroy VM",
      });
    }
  }, [flowId]);

  const handleRecreate = useCallback(async () => {
    try {
      setVmState({ status: "creating" });
      const vm = await api.createFlowVm(flowId);
      setVmState({ status: "ready", vm });
    } catch (e) {
      setVmState({
        status: "error",
        message: (e as Error).message || "Failed to create VM",
      });
    }
  }, [flowId]);

  // ── Render ──────────────────────────────────────────────────────

  if (vmState.status === "loading") {
    return (
      <div className="vm-terminal-status">
        <span className="vm-terminal-spinner" />
        Checking VM for {nodeLabel}...
      </div>
    );
  }

  if (vmState.status === "creating") {
    return (
      <div className="vm-terminal-status">
        <span className="vm-terminal-spinner" />
        Creating VM for {nodeLabel}... This may take a few seconds.
      </div>
    );
  }

  if (vmState.status === "error") {
    return (
      <div className="vm-terminal-status error">
        <span className="vm-terminal-error-icon">!</span>
        <span>{vmState.message}</span>
        <button className="vm-terminal-btn" onClick={handleRecreate}>
          Retry
        </button>
      </div>
    );
  }

  if (vmState.status === "destroyed") {
    return (
      <div className="vm-terminal-status">
        <span>VM destroyed.</span>
        <button className="vm-terminal-btn" onClick={handleRecreate}>
          Create New VM
        </button>
      </div>
    );
  }

  // status === "ready"
  const { vm } = vmState;

  return (
    <div className="vm-terminal-container">
      <div className="vm-terminal-infobar">
        <span className="vm-terminal-dot running" />
        <span className="vm-terminal-info">
          VM #{vm.vm_id} &middot; {vm.tier} &middot; {vm.guest_ip}
        </span>
        <span className="vm-terminal-info secondary">
          SSH: <code>{vm.ssh_command}</code>
        </span>
        <div style={{ flex: 1 }} />
        <button
          className="vm-terminal-btn danger"
          onClick={handleDestroy}
          title="Destroy this VM"
        >
          Destroy VM
        </button>
      </div>
      <iframe
        className="vm-terminal-iframe"
        src={vm.web_terminal}
        title={`VM Terminal - ${nodeLabel}`}
      />
    </div>
  );
}
