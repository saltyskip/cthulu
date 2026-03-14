import { useState, useEffect, useMemo } from "react";
import * as api from "../api/client";

interface SetupScreenProps {
  onComplete: () => void;
}

export default function SetupScreen({ onComplete }: SetupScreenProps) {
  // -- GitHub PAT --
  const [pat, setPat] = useState("");
  const [patSaving, setPatSaving] = useState(false);
  const [patSaved, setPatSaved] = useState(false);
  const [patError, setPatError] = useState("");
  const [patUsername, setPatUsername] = useState("");

  // -- Claude OAuth (read-only) --
  const [claudeOk, setClaudeOk] = useState<boolean | null>(null);

  // -- Anthropic API Key (required) --
  const [anthropicKey, setAnthropicKey] = useState("");
  const [anthropicSaving, setAnthropicSaving] = useState(false);
  const [anthropicSaved, setAnthropicSaved] = useState(false);

  // -- Slack Webhook URL (required) --
  const [slackUrl, setSlackUrl] = useState("");
  const [slackSaving, setSlackSaving] = useState(false);
  const [slackSaved, setSlackSaved] = useState(false);

  // -- OpenAI API Key (optional) --
  const [openaiKey, setOpenaiKey] = useState("");
  const [openaiSaving, setOpenaiSaving] = useState(false);
  const [openaiSaved, setOpenaiSaved] = useState(false);

  // -- Notion (optional) --
  const [notionToken, setNotionToken] = useState("");
  const [notionDbId, setNotionDbId] = useState("");
  const [notionSaving, setNotionSaving] = useState(false);
  const [notionSaved, setNotionSaved] = useState(false);

  // -- Telegram (optional) --
  const [tgBotToken, setTgBotToken] = useState("");
  const [tgChatId, setTgChatId] = useState("");
  const [tgSaving, setTgSaving] = useState(false);
  const [tgSaved, setTgSaved] = useState(false);

  // -- Error state (shared) --
  const [error, setError] = useState("");

  useEffect(() => {
    api.checkSetupStatus().then((status) => {
      setPatSaved(status.github_pat_configured);
      setClaudeOk(status.claude_oauth_configured);
      setAnthropicSaved(status.anthropic_api_key_configured);
      setSlackSaved(status.slack_webhook_configured);
      setOpenaiSaved(status.openai_api_key_configured);
      setNotionSaved(status.notion_configured);
      setTgSaved(status.telegram_configured);
    });
  }, []);

  const allRequiredDone = useMemo(
    () => patSaved && anthropicSaved && slackSaved,
    [patSaved, anthropicSaved, slackSaved]
  );

  const requiredCount = useMemo(() => {
    let n = 0;
    if (patSaved) n++;
    if (anthropicSaved) n++;
    if (slackSaved) n++;
    return n;
  }, [patSaved, anthropicSaved, slackSaved]);

  // -- Handlers --
  const handleSavePat = async () => {
    setPatSaving(true);
    setError("");
    setPatError("");
    try {
      const result = await api.saveGithubPat(pat);
      if (result.ok) {
        setPatSaved(true);
        setPatUsername(result.username);
        try { await api.setupAgentRepo(); } catch { /* non-fatal */ }
      } else {
        setPatError("Invalid token -- could not authenticate with GitHub.");
      }
    } catch (e) {
      setPatError(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    } finally {
      setPatSaving(false);
    }
  };

  const handleSaveAnthropicKey = async () => {
    setAnthropicSaving(true);
    setError("");
    try {
      await api.saveAnthropicKey(anthropicKey);
      setAnthropicSaved(true);
    } catch (e) {
      setError(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    } finally {
      setAnthropicSaving(false);
    }
  };

  const handleSaveSlack = async () => {
    setSlackSaving(true);
    setError("");
    try {
      await api.saveSlackWebhook(slackUrl);
      setSlackSaved(true);
    } catch (e) {
      setError(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    } finally {
      setSlackSaving(false);
    }
  };

  const handleSaveOpenai = async () => {
    setOpenaiSaving(true);
    setError("");
    try {
      await api.saveOpenaiKey(openaiKey);
      setOpenaiSaved(true);
    } catch (e) {
      setError(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    } finally {
      setOpenaiSaving(false);
    }
  };

  const handleSaveNotion = async () => {
    setNotionSaving(true);
    setError("");
    try {
      await api.saveNotionCredentials(notionToken, notionDbId);
      setNotionSaved(true);
    } catch (e) {
      setError(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    } finally {
      setNotionSaving(false);
    }
  };

  const handleSaveTelegram = async () => {
    setTgSaving(true);
    setError("");
    try {
      await api.saveTelegramCredentials(tgBotToken, tgChatId);
      setTgSaved(true);
    } catch (e) {
      setError(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    } finally {
      setTgSaving(false);
    }
  };

  return (
    <div className="setup-screen">
      <div className="setup-card">
        <h1 className="setup-title">Welcome to Cthulu Studio</h1>
        <p className="setup-subtitle">
          Configure your credentials to get started.
        </p>

        {/* Progress indicator */}
        <div className="setup-progress">
          <span className="setup-progress-text">
            {requiredCount} of 3 required credentials configured
          </span>
          <div className="setup-progress-bar">
            <div
              className="setup-progress-fill"
              style={{ width: `${(requiredCount / 3) * 100}%` }}
            />
          </div>
        </div>

        {/* ── REQUIRED SECTION ── */}
        <div className="setup-group-label">Required</div>

        {/* 1. GitHub PAT (required) */}
        <div className="setup-section">
          <h2>
            GitHub Personal Access Token
            <span className="setup-required">Required</span>
          </h2>
          <p className="setup-hint">
            Used for workflow sync and GitHub integrations. Create one at{" "}
            <a href="https://github.com/settings/tokens" target="_blank" rel="noreferrer">
              github.com/settings/tokens
            </a>{" "}
            with <code>repo</code> scope.
          </p>
          {patSaved ? (
            <div className="setup-success">
              Authenticated{patUsername ? ` as ${patUsername}` : ""}
            </div>
          ) : (
            <div className="setup-input-row">
              <input
                type="password"
                value={pat}
                onChange={(e) => setPat(e.target.value)}
                placeholder="ghp_xxxxxxxxxxxxxxxxxxxx"
                className="setup-input"
              />
              <button
                onClick={handleSavePat}
                disabled={patSaving || pat.length < 10}
                className="setup-btn"
              >
                {patSaving ? "Validating..." : "Validate & Save"}
              </button>
            </div>
          )}
          {patError && <div className="setup-error">{patError}</div>}
        </div>

        {/* 2. Anthropic API Key (required) */}
        <div className="setup-section">
          <h2>
            Anthropic API Key
            <span className="setup-required">Required</span>
          </h2>
          <p className="setup-hint">
            Powers agent AI capabilities. Get yours at{" "}
            <a href="https://console.anthropic.com/settings/keys" target="_blank" rel="noreferrer">
              console.anthropic.com
            </a>.
          </p>
          {anthropicSaved ? (
            <div className="setup-success">API key saved</div>
          ) : (
            <div className="setup-input-row">
              <input
                type="password"
                value={anthropicKey}
                onChange={(e) => setAnthropicKey(e.target.value)}
                placeholder="sk-ant-xxxxxxxxxxxxxxxxxxxx"
                className="setup-input"
              />
              <button
                onClick={handleSaveAnthropicKey}
                disabled={anthropicSaving || anthropicKey.length < 10}
                className="setup-btn"
              >
                {anthropicSaving ? "Saving..." : "Save"}
              </button>
            </div>
          )}
        </div>

        {/* 3. Slack Webhook URL (required) */}
        <div className="setup-section">
          <h2>
            Slack Webhook URL
            <span className="setup-required">Required</span>
          </h2>
          <p className="setup-hint">
            Used by workflow sinks to post messages. Create one at{" "}
            <a href="https://api.slack.com/messaging/webhooks" target="_blank" rel="noreferrer">
              api.slack.com/messaging/webhooks
            </a>.
          </p>
          {slackSaved ? (
            <div className="setup-success">Webhook URL saved</div>
          ) : (
            <div className="setup-input-row">
              <input
                type="password"
                value={slackUrl}
                onChange={(e) => setSlackUrl(e.target.value)}
                placeholder="https://hooks.slack.com/services/T.../B.../xxx"
                className="setup-input"
              />
              <button
                onClick={handleSaveSlack}
                disabled={slackSaving || slackUrl.length < 20}
                className="setup-btn"
              >
                {slackSaving ? "Saving..." : "Save"}
              </button>
            </div>
          )}
        </div>

        {/* ── OPTIONAL SECTION ── */}
        <div className="setup-group-label">Optional</div>

        {/* 4. Claude OAuth (read-only) */}
        <div className="setup-section">
          <h2>
            Claude Authentication
            <span className="setup-optional">Read-only</span>
          </h2>
          {claudeOk === null ? (
            <p className="setup-hint">Checking...</p>
          ) : claudeOk ? (
            <div className="setup-success">
              Claude CLI authenticated via macOS Keychain
            </div>
          ) : (
            <div className="setup-warning">
              Not authenticated. Run <code>claude login</code> in your terminal,
              then restart the app.
            </div>
          )}
        </div>

        {/* 5. OpenAI API Key (optional) */}
        <div className="setup-section">
          <h2>
            OpenAI API Key
            <span className="setup-optional">Optional</span>
          </h2>
          <p className="setup-hint">
            Stored for future use. Not currently used by any workflow step.
          </p>
          {openaiSaved ? (
            <div className="setup-success">API key saved</div>
          ) : (
            <div className="setup-input-row">
              <input
                type="password"
                value={openaiKey}
                onChange={(e) => setOpenaiKey(e.target.value)}
                placeholder="sk-xxxxxxxxxxxxxxxxxxxx"
                className="setup-input"
              />
              <button
                onClick={handleSaveOpenai}
                disabled={openaiSaving || openaiKey.length < 10}
                className="setup-btn setup-btn-secondary"
              >
                {openaiSaving ? "Saving..." : "Save"}
              </button>
            </div>
          )}
        </div>

        {/* 6. Notion (optional) */}
        <div className="setup-section">
          <h2>
            Notion Integration
            <span className="setup-optional">Optional</span>
          </h2>
          <p className="setup-hint">
            Used by Notion sinks to create pages. Get a token at{" "}
            <a href="https://www.notion.so/my-integrations" target="_blank" rel="noreferrer">
              notion.so/my-integrations
            </a>.
          </p>
          {notionSaved ? (
            <div className="setup-success">Notion credentials saved</div>
          ) : (
            <>
              <div className="setup-input-row" style={{ marginBottom: 6 }}>
                <input
                  type="password"
                  value={notionToken}
                  onChange={(e) => setNotionToken(e.target.value)}
                  placeholder="ntn_xxxxxxxxxxxxxxxxxxxx"
                  className="setup-input"
                />
              </div>
              <div className="setup-input-row">
                <input
                  type="text"
                  value={notionDbId}
                  onChange={(e) => setNotionDbId(e.target.value)}
                  placeholder="Database ID (32-hex UUID)"
                  className="setup-input"
                />
                <button
                  onClick={handleSaveNotion}
                  disabled={notionSaving || notionToken.length < 10 || notionDbId.length < 10}
                  className="setup-btn setup-btn-secondary"
                >
                  {notionSaving ? "Saving..." : "Save"}
                </button>
              </div>
            </>
          )}
        </div>

        {/* 7. Telegram (optional) */}
        <div className="setup-section">
          <h2>
            Telegram Bot
            <span className="setup-optional">Optional</span>
          </h2>
          <p className="setup-hint">
            Send workflow outputs to Telegram. Create a bot via{" "}
            <a href="https://t.me/BotFather" target="_blank" rel="noreferrer">
              @BotFather
            </a>, then use <code>/start</code> in the target chat.
          </p>
          {tgSaved ? (
            <div className="setup-success">Telegram credentials saved</div>
          ) : (
            <>
              <div className="setup-input-row" style={{ marginBottom: 6 }}>
                <input
                  type="password"
                  value={tgBotToken}
                  onChange={(e) => setTgBotToken(e.target.value)}
                  placeholder="123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"
                  className="setup-input"
                />
              </div>
              <div className="setup-input-row">
                <input
                  type="text"
                  value={tgChatId}
                  onChange={(e) => setTgChatId(e.target.value)}
                  placeholder="Chat ID (e.g. -1001234567890)"
                  className="setup-input"
                />
                <button
                  onClick={handleSaveTelegram}
                  disabled={tgSaving || tgBotToken.length < 10 || tgChatId.length < 3}
                  className="setup-btn setup-btn-secondary"
                >
                  {tgSaving ? "Saving..." : "Save"}
                </button>
              </div>
            </>
          )}
        </div>

        {/* Error display */}
        {error && <div className="setup-error">{error}</div>}

        {/* Continue button */}
        <button
          className="setup-btn setup-btn-primary setup-continue"
          disabled={!allRequiredDone}
          onClick={onComplete}
        >
          Continue to Cthulu Studio
        </button>
        {!allRequiredDone && (
          <p className="setup-footer-hint">
            Complete all required fields above to continue.
          </p>
        )}
      </div>
    </div>
  );
}
