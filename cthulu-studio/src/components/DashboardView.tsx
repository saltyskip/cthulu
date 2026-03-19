import { useState, useEffect, useCallback, useMemo } from "react";
import {
  getDashboardConfig,
  saveDashboardConfig,
  getDashboardMessages,
  getDashboardSummary,
  type DashboardConfig,
  type SlackChannelMessages,
  type SlackMessage,
  type ChannelSummary,
} from "../api/client";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

function getGreeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return "Good morning";
  if (hour < 17) return "Good afternoon";
  return "Good evening";
}

function formatDate(): string {
  return new Date().toLocaleDateString("en-US", {
    weekday: "long",
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

function formatTime(isoTime: string): string {
  try {
    const d = new Date(isoTime);
    return d.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit", hour12: false });
  } catch {
    return "";
  }
}

function ThreadReplies({ replies }: { replies: SlackMessage[] }) {
  const [expanded, setExpanded] = useState(false);

  if (replies.length === 0) return null;

  return (
    <div className="dashboard-thread">
      <button
        className="dashboard-thread-toggle"
        onClick={() => setExpanded((prev) => !prev)}
      >
        {expanded ? "\u25BE" : "\u25B8"} {replies.length} repl{replies.length === 1 ? "y" : "ies"}
      </button>
      {expanded && (
        <div className="dashboard-thread-replies">
          {replies.map((reply) => (
            <div key={reply.ts} className="dashboard-message dashboard-thread-reply">
              <span className="dashboard-msg-time">{formatTime(reply.time)}</span>
              <span className="dashboard-msg-user">{reply.user}</span>
              <span className="dashboard-msg-text">{reply.text}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export default function DashboardView() {
  const [config, setConfig] = useState<DashboardConfig | null>(null);
  const [channels, setChannels] = useState<SlackChannelMessages[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showChannelDialog, setShowChannelDialog] = useState(false);
  const [channelInput, setChannelInput] = useState("");
  const [fetchedAt, setFetchedAt] = useState<string | null>(null);

  // Summary state
  const [summaries, setSummaries] = useState<ChannelSummary[]>([]);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);

  // Map summaries by channel name for quick lookup.
  // Normalize keys: strip leading # and lowercase to handle mismatches
  // between Slack channel names and Claude's response format.
  const summaryMap = useMemo(() => {
    const map = new Map<string, string>();
    for (const s of summaries) {
      const key = s.channel.replace(/^#/, "").toLowerCase();
      map.set(key, s.summary);
    }
    return map;
  }, [summaries]);

  const getSummaryForChannel = useCallback((channelName: string): string | undefined => {
    const key = channelName.replace(/^#/, "").replace(/^\u{1F512}/u, "").toLowerCase();
    // Try exact normalized match first
    const exact = summaryMap.get(key);
    if (exact) return exact;
    // If there's an "all" fallback (raw text from Claude), use that
    const fallback = summaryMap.get("all");
    if (fallback && summaryMap.size === 1) return fallback;
    return undefined;
  }, [summaryMap]);

  const loadConfig = useCallback(async () => {
    try {
      const cfg = await getDashboardConfig();
      setConfig(cfg);
      return cfg;
    } catch (e) {
      setError(`Failed to load config: ${(e as Error).message}`);
      return null;
    }
  }, []);

  const loadMessages = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getDashboardMessages();
      setChannels(data.channels || []);
      setFetchedAt(data.fetched_at || null);
    } catch (e) {
      const msg = (e as Error).message;
      // Don't show error if just no channels configured
      if (!msg.includes("No channels configured")) {
        setError(msg);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  const loadSummary = useCallback(async (channelData: SlackChannelMessages[]) => {
    if (channelData.length === 0) return;
    setSummaryLoading(true);
    setSummaryError(null);
    try {
      const data = await getDashboardSummary(channelData);
      setSummaries(data.summaries || []);
    } catch (e) {
      setSummaryError(`Summary failed: ${(e as Error).message}`);
    } finally {
      setSummaryLoading(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const cfg = await loadConfig();
      if (cancelled) return;
      if (cfg?.first_run) {
        setShowChannelDialog(true);
        setLoading(false);
      } else if (cfg && cfg.channels.length > 0) {
        await loadMessages();
      } else {
        setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [loadConfig, loadMessages]);

  const handleSaveChannels = async () => {
    const names = channelInput
      .split(",")
      .map((s) => s.trim().replace(/^#/, ""))
      .filter(Boolean);
    if (names.length === 0) return;

    try {
      await saveDashboardConfig(names);
      setShowChannelDialog(false);
      // Clear old summaries when channels change
      setSummaries([]);
      const cfg = await loadConfig();
      if (cfg && cfg.channels.length > 0) {
        await loadMessages();
      }
    } catch (e) {
      setError(`Failed to save config: ${(e as Error).message}`);
    }
  };

  const handleRefresh = async () => {
    setSummaries([]);
    await loadMessages();
  };

  const handleSummarize = () => {
    loadSummary(channels);
  };

  const totalMessages = useMemo(
    () => channels.reduce((sum, ch) => sum + ch.count, 0),
    [channels],
  );

  return (
    <div className="dashboard-view">
      <div className="dashboard-header">
        <h1 className="dashboard-greeting">{getGreeting()}</h1>
        <p className="dashboard-date">{formatDate()}</p>
      </div>

      <div className="dashboard-content">
        <div className="dashboard-section-header">
          <h2>Today&apos;s Messages</h2>
          <div className="dashboard-actions">
            {fetchedAt && (
              <span className="dashboard-fetched-at">
                {formatTime(fetchedAt)}
              </span>
            )}
            <button className="dashboard-btn" onClick={handleRefresh} disabled={loading}>
              {loading ? "Loading..." : "Refresh"}
            </button>
            <button
              className="dashboard-btn dashboard-btn-summarize"
              onClick={handleSummarize}
              disabled={summaryLoading || channels.length === 0}
            >
              {summaryLoading ? "Summarizing..." : "Summarize"}
            </button>
            <button
              className="dashboard-btn"
              onClick={() => {
                setChannelInput(config?.channels.join(", ") || "");
                setShowChannelDialog(true);
              }}
            >
              Channels
            </button>
          </div>
        </div>

        {error && <div className="dashboard-error">{error}</div>}
        {summaryError && <div className="dashboard-error">{summaryError}</div>}

        {loading && channels.length === 0 && !error && (
          <div className="dashboard-loading">Fetching messages from Slack...</div>
        )}

        {!loading && channels.length === 0 && !error && (
          <div className="dashboard-empty">
            {config?.first_run || config?.channels.length === 0
              ? "No channels configured yet. Click 'Channels' to get started."
              : "No messages found for today."}
          </div>
        )}

        {totalMessages > 0 && (
          <div className="dashboard-stats">
            {totalMessages} message{totalMessages !== 1 ? "s" : ""} across {channels.length} channel{channels.length !== 1 ? "s" : ""}
          </div>
        )}

        <div className="dashboard-channels">
          {channels.map((ch) => {
            const channelSummary = getSummaryForChannel(ch.channel);
            return (
              <div key={ch.channel} className="dashboard-channel">
                <div className="dashboard-channel-header">
                  <span className="dashboard-channel-name">{ch.channel}</span>
                  <span className="dashboard-channel-count">{ch.count}</span>
                </div>
                {channelSummary && (
                  <div className="dashboard-channel-summary">
                    <span className="dashboard-channel-summary-label">AI Summary</span>
                    <p className="dashboard-channel-summary-text">{channelSummary}</p>
                  </div>
                )}
                <div className="dashboard-messages">
                  {ch.messages.map((msg) => (
                    <div key={msg.ts}>
                      <div className="dashboard-message">
                        <span className="dashboard-msg-time">{formatTime(msg.time)}</span>
                        <span className="dashboard-msg-user">{msg.user}</span>
                        <span className="dashboard-msg-text">{msg.text}</span>
                      </div>
                      {msg.replies && msg.replies.length > 0 && (
                        <ThreadReplies replies={msg.replies} />
                      )}
                    </div>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <Dialog open={showChannelDialog} onOpenChange={setShowChannelDialog}>
        <DialogContent className="cth-dialog">
          <DialogHeader>
            <DialogTitle>Slack Channels</DialogTitle>
          </DialogHeader>
          <div className="cth-dialog-field">
            <label className="cth-dialog-label">
              Enter channel names (comma-separated, without #)
            </label>
            <input
              value={channelInput}
              onChange={(e) => setChannelInput(e.target.value)}
              placeholder="general, devops, engineering"
              className="cth-dialog-input"
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSaveChannels();
              }}
              autoFocus
            />
            {config && config.channels.length > 0 && (
              <p className="cth-dialog-hint">
                Current: {config.channels.map((c) => `#${c}`).join(", ")}
              </p>
            )}
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setShowChannelDialog(false)}>
              Cancel
            </Button>
            <Button onClick={handleSaveChannels}>Save</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
