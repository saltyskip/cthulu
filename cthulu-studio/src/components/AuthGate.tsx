import { createContext, useContext, useState, useCallback, useEffect, type ReactNode } from "react";
import { getServerUrl, setAuthTokenGetter, getProfile, updateProfile, type UserProfile } from "../api/client";

interface AuthUser { id: string; email: string; }
interface AuthContextValue { user: AuthUser | null; token: string | null; logout: () => void; }

export const AuthContext = createContext<AuthContextValue>({ user: null, token: null, logout: () => {} });
export function useAuth(): AuthContextValue { return useContext(AuthContext); }

function decodeJwtPayload(token: string): Record<string, unknown> | null {
  try { const p = token.split("."); if (p.length !== 3) return null; return JSON.parse(atob(p[1].replace(/-/g, "+").replace(/_/g, "/"))); } catch { return null; }
}
function isTokenValid(token: string): { valid: boolean; user: AuthUser | null } {
  const p = decodeJwtPayload(token);
  if (!p) return { valid: false, user: null };
  const email = p.email as string | undefined, exp = p.exp as number | undefined, sub = p.sub as string | undefined;
  if (!email || !exp || Date.now() / 1000 > exp) return { valid: false, user: null };
  return { valid: true, user: { id: sub || "", email } };
}

const TOKEN_KEY = "cthulu_auth_token";
const GOOGLE_CLIENT_ID = import.meta.env.VITE_GOOGLE_CLIENT_ID || "";

export default function AuthGate({ children }: { children: ReactNode }) {
  const authEnabled = import.meta.env.VITE_AUTH_ENABLED === "true";
  const [token, setToken] = useState<string | null>(() => {
    const stored = localStorage.getItem(TOKEN_KEY);
    if (!stored) return null;
    const { valid } = isTokenValid(stored);
    if (!valid) { localStorage.removeItem(TOKEN_KEY); return null; }
    return stored;
  });
  const [user, setUser] = useState<AuthUser | null>(() => token ? isTokenValid(token).user : null);
  const [needsOnboarding, setNeedsOnboarding] = useState(false);
  const [profile, setProfile] = useState<UserProfile | null>(null);

  useEffect(() => { setAuthTokenGetter(() => token); }, [token]);

  // Check if returning user has VM already set up
  useEffect(() => {
    if (!token || !user) return;
    // Check profile + VM status
    getProfile().then(async (p) => {
      setProfile(p);
      if (p.vm_id != null) {
        // User has a VM — verify it's still running
        try {
          const res = await fetch(`/api/auth/vm-status?token=${encodeURIComponent(localStorage.getItem(TOKEN_KEY) || "")}`);
          const status = await res.json();
          if (status.has_vm && status.running) {
            // VM alive → skip onboarding
            setNeedsOnboarding(false);
          } else {
            // VM dead or deleted → re-onboard
            setNeedsOnboarding(true);
          }
        } catch {
          // Can't check → assume OK
          setNeedsOnboarding(false);
        }
      } else {
        // No VM → show onboarding
        setNeedsOnboarding(true);
      }
    }).catch(() => setNeedsOnboarding(true));
  }, [token, user]);

  const logout = useCallback(() => { localStorage.removeItem(TOKEN_KEY); setToken(null); setUser(null); setProfile(null); }, []);
  const handleAuth = useCallback((jwt: string, authUser: AuthUser) => { localStorage.setItem(TOKEN_KEY, jwt); setToken(jwt); setUser(authUser); }, []);
  const handleOnboardingComplete = useCallback(() => { setNeedsOnboarding(false); }, []);

  if (!authEnabled) return <AuthContext.Provider value={{ user: null, token: null, logout }}>{children}</AuthContext.Provider>;

  if (token && user && needsOnboarding) {
    return (
      <AuthContext.Provider value={{ user, token, logout }}>
        <OnboardingScreen user={user} profile={profile} onComplete={handleOnboardingComplete} onLogout={logout} />
      </AuthContext.Provider>
    );
  }
  if (token && user) return <AuthContext.Provider value={{ user, token, logout }}>{children}</AuthContext.Provider>;
  return <AuthContext.Provider value={{ user: null, token: null, logout }}><AuthForm onAuth={handleAuth} /></AuthContext.Provider>;
}

// ── Onboarding ────────────────────────────────────────────

function OnboardingScreen({ user, profile, onComplete, onLogout }: {
  user: AuthUser; profile: UserProfile | null; onComplete: () => void; onLogout: () => void;
}) {
  const [step, setStep] = useState(1);
  const [slackWebhook, setSlackWebhook] = useState("");
  const [provisioningVm, setProvisioningVm] = useState(false);
  const [vmStatus, setVmStatus] = useState<string | null>(null);
  const [terminalUrl, setTerminalUrl] = useState<string | null>(null);
  const [promptsCopied, setPromptsCopied] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const handleProvisionVm = () => {
    setProvisioningVm(true);
    setError(null);
    setVmStatus("Connecting to VM Manager...");

    const token = localStorage.getItem(TOKEN_KEY) || "";
    const source = new EventSource(`/api/auth/provision-vm?token=${encodeURIComponent(token)}`);

    source.addEventListener("progress", (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data);
        setVmStatus(data.message);
      } catch { /* ignore */ }
    });

    source.addEventListener("done", (e: MessageEvent) => {
      source.close();
      setProvisioningVm(false);
      try {
        const data = JSON.parse(e.data);
        if (data.status === "already_exists") {
          setVmStatus(`VM #${data.vm_id} already assigned`);
        } else {
          setVmStatus(`VM #${data.vm_id} created!`);
        }
        if (data.terminal_url) setTerminalUrl(data.terminal_url);
        if (data.prompts_copied) setPromptsCopied(data.prompts_copied);
        setStep(2);
      } catch { /* ignore */ }
    });

    source.addEventListener("error", (e: MessageEvent) => {
      source.close();
      setProvisioningVm(false);
      try {
        const data = JSON.parse(e.data);
        setError(data.message);
      } catch {
        setError("VM creation failed");
      }
    });

    source.onerror = () => {
      source.close();
      setProvisioningVm(false);
      if (!error) setError("Connection lost during VM creation");
    };
  };

  const [savingSlack, setSavingSlack] = useState(false);

  const handleSaveSlack = async () => {
    if (slackWebhook.trim()) {
      setSavingSlack(true);
      try {
        const token = localStorage.getItem(TOKEN_KEY) || "";
        await fetch("/api/auth/vm-env", {
          method: "POST",
          headers: { "Content-Type": "application/json", "Authorization": `Bearer ${token}` },
          body: JSON.stringify({ env: { SLACK_WEBHOOK_URL: slackWebhook.trim() } }),
        });
      } catch { /* best effort */ }
      setSavingSlack(false);
    }
    onComplete();
  };

  return (
    <div className="auth-gate">
      <div className="auth-card" style={{ maxWidth: 480 }}>
        <h1>Welcome to Cthulu</h1>
        <p className="auth-subtitle">Hi {user.email} — let's set up your workspace.</p>

        {/* Step indicator */}
        <div style={{ display: "flex", gap: 8, marginBottom: 24 }}>
          {[1, 2, 3].map((s) => (
            <div key={s} style={{
              flex: 1, height: 4, borderRadius: 2,
              background: s <= step ? "var(--accent)" : "var(--border)",
            }} />
          ))}
        </div>

        {step === 1 && (
          <div>
            <h3 style={{ fontSize: 15, fontWeight: 600, marginBottom: 12 }}>Step 1: Create your VM</h3>
            <p style={{ fontSize: 13, color: "var(--muted)", marginBottom: 16 }}>
              We'll spin up a dedicated virtual machine for you. All your AI agents and workflows will run here — fully isolated.
            </p>
            {error && <div className="auth-error">{error}</div>}
            {vmStatus && <div style={{ fontSize: 13, color: "var(--success)", marginBottom: 12 }}>{vmStatus}</div>}
            <button className="auth-btn" onClick={handleProvisionVm} disabled={provisioningVm}>
              {provisioningVm ? "Creating VM..." : "Create My VM"}
            </button>
            <button className="auth-btn" style={{ background: "transparent", color: "var(--fg)", border: "1px solid var(--border)", marginTop: 8 }} onClick={() => setStep(2)}>
              Skip (I'll set up later)
            </button>
          </div>
        )}

        {step === 2 && (
          <div>
            <h3 style={{ fontSize: 15, fontWeight: 600, marginBottom: 12 }}>Step 2: Login to Claude Code on your VM</h3>
            {vmStatus && <div style={{ fontSize: 13, color: "var(--success)", marginBottom: 12 }}>{vmStatus}</div>}

            {terminalUrl && (
              <div style={{ marginBottom: 16 }}>
                <p style={{ fontSize: 13, color: "var(--muted)", marginBottom: 8 }}>
                  Open your VM terminal and run <code style={{ background: "var(--bg)", padding: "2px 6px", borderRadius: 4 }}>claude auth login</code>:
                </p>
                <a
                  href={terminalUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="auth-btn"
                  style={{ display: "block", textAlign: "center", textDecoration: "none", marginBottom: 12 }}
                >
                  Open Web Terminal
                </a>
              </div>
            )}

            {!terminalUrl && (
              <div style={{ background: "var(--bg)", padding: 12, borderRadius: 6, fontFamily: "monospace", fontSize: 13, marginBottom: 16, border: "1px solid var(--border)" }}>
                <code>claude auth login</code>
              </div>
            )}

            <p style={{ fontSize: 12, color: "var(--muted)", marginBottom: 16 }}>
              Follow the browser link to authenticate with your Anthropic account.
            </p>

            {promptsCopied.length > 0 && (
              <div style={{ background: "var(--bg)", borderRadius: 8, padding: 12, marginBottom: 16, border: "1px solid var(--border)" }}>
                <h4 style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>Workflow Prompts Installed</h4>
                <p style={{ fontSize: 12, color: "var(--muted)", marginBottom: 8 }}>
                  These prompt files are on your VM at <code style={{ background: "var(--surface)", padding: "1px 4px", borderRadius: 3 }}>/root/prompts/</code>.
                  Each one powers a workflow — the executor node reads the prompt and sends it to Claude.
                </p>
                <div style={{ fontSize: 12, fontFamily: "monospace" }}>
                  {promptsCopied.map((f) => (
                    <div key={f} style={{ padding: "2px 0", color: "var(--fg)" }}>
                      /root/prompts/{f}
                    </div>
                  ))}
                </div>
                <p style={{ fontSize: 11, color: "var(--muted)", marginTop: 8 }}>
                  To add your own: create a <code>.md</code> file in <code>/root/prompts/</code> on your VM, then reference it in a workflow executor node.
                </p>
              </div>
            )}

            <button className="auth-btn" onClick={() => setStep(3)}>Done — Next</button>
          </div>
        )}

        {step === 3 && (
          <div>
            <h3 style={{ fontSize: 15, fontWeight: 600, marginBottom: 12 }}>Step 3: Slack Webhook (optional)</h3>
            <p style={{ fontSize: 13, color: "var(--muted)", marginBottom: 12 }}>
              Add a Slack webhook URL to receive workflow notifications.
            </p>
            <div className="auth-field">
              <label htmlFor="slack-webhook">Slack Webhook URL</label>
              <input
                id="slack-webhook"
                value={slackWebhook}
                onChange={(e) => setSlackWebhook(e.target.value)}
                placeholder="https://hooks.slack.com/services/..."
              />
            </div>
            <button className="auth-btn" onClick={handleSaveSlack} disabled={savingSlack}>
              {savingSlack ? "Saving to VM..." : slackWebhook.trim() ? "Save & Start" : "Skip & Start"}
            </button>
          </div>
        )}

        <div style={{ textAlign: "center", marginTop: 16 }}>
          <a onClick={onLogout} style={{ fontSize: 12, color: "var(--muted)", cursor: "pointer" }}>Log out</a>
        </div>
      </div>
    </div>
  );
}

// ── Auth Form ────────────────────────────────────────────

function AuthForm({ onAuth }: { onAuth: (token: string, user: AuthUser) => void }) {
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault(); setError(null); setLoading(true);
    try {
      const res = await fetch(`${getServerUrl()}/api/auth/${mode === "login" ? "login" : "signup"}`, {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password }),
      });
      const data = await res.json();
      if (!res.ok) { setError(data.error || `Error ${res.status}`); return; }
      onAuth(data.token, data.user);
    } catch (err) { setError((err as Error).message); } finally { setLoading(false); }
  };

  const handleGoogleLogin = () => {
    if (!GOOGLE_CLIENT_ID) { setError("Google login not configured"); return; }
    const redirectUri = `${window.location.origin}/auth/google/callback`;
    const params = new URLSearchParams({
      client_id: GOOGLE_CLIENT_ID, redirect_uri: redirectUri, response_type: "code",
      scope: "openid email profile", access_type: "offline", prompt: "select_account",
    });
    localStorage.setItem("google_oauth_redirect_uri", redirectUri);
    window.location.href = `https://accounts.google.com/o/oauth2/v2/auth?${params}`;
  };

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const code = params.get("code");
    if (!code) return;
    const redirectUri = localStorage.getItem("google_oauth_redirect_uri") || `${window.location.origin}/auth/google/callback`;
    window.history.replaceState({}, "", "/");
    localStorage.removeItem("google_oauth_redirect_uri");
    setLoading(true); setError(null);
    fetch(`${getServerUrl()}/api/auth/google`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code, redirect_uri: redirectUri }),
    }).then(r => r.json()).then(data => {
      if (data.error) setError(data.error); else onAuth(data.token, data.user);
    }).catch(err => setError((err as Error).message)).finally(() => setLoading(false));
  }, [onAuth]);

  return (
    <div className="auth-gate">
      <form className="auth-card" onSubmit={handleSubmit}>
        <h1>{mode === "login" ? "Welcome back" : "Create account"}</h1>
        <p className="auth-subtitle">{mode === "login" ? "Sign in to Cthulu Studio" : "Sign up for Cthulu Studio"}</p>
        {GOOGLE_CLIENT_ID && (
          <>
            <button type="button" className="auth-btn auth-btn-google" onClick={handleGoogleLogin} disabled={loading}>
              <svg width="18" height="18" viewBox="0 0 18 18" style={{ marginRight: 8, verticalAlign: "middle" }}>
                <path fill="#4285F4" d="M17.64 9.2c0-.63-.06-1.25-.16-1.84H9v3.49h4.84a4.14 4.14 0 0 1-1.8 2.71v2.26h2.92a8.78 8.78 0 0 0 2.68-6.62z"/>
                <path fill="#34A853" d="M9 18c2.43 0 4.47-.8 5.96-2.18l-2.92-2.26c-.8.54-1.83.86-3.04.86-2.34 0-4.32-1.58-5.03-3.71H.96v2.33A8.99 8.99 0 0 0 9 18z"/>
                <path fill="#FBBC05" d="M3.97 10.71A5.41 5.41 0 0 1 3.69 9c0-.59.1-1.17.28-1.71V4.96H.96A8.99 8.99 0 0 0 0 9c0 1.45.35 2.82.96 4.04l3.01-2.33z"/>
                <path fill="#EA4335" d="M9 3.58c1.32 0 2.5.45 3.44 1.35l2.58-2.59C13.46.89 11.43 0 9 0A8.99 8.99 0 0 0 .96 4.96l3.01 2.33C4.68 5.16 6.66 3.58 9 3.58z"/>
              </svg>
              Continue with Google
            </button>
            <div className="auth-divider"><span>or</span></div>
          </>
        )}
        <div className="auth-field"><label htmlFor="auth-email">Email</label><input id="auth-email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="you@example.com" required autoFocus /></div>
        <div className="auth-field"><label htmlFor="auth-password">Password</label><input id="auth-password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder={mode === "signup" ? "Min 8 characters" : ""} required minLength={mode === "signup" ? 8 : undefined} /></div>
        {error && <div className="auth-error">{error}</div>}
        <button type="submit" className="auth-btn" disabled={loading}>{loading ? "..." : mode === "login" ? "Sign in" : "Sign up"}</button>
        <div className="auth-toggle">{mode === "login" ? (<>Don&apos;t have an account? <a onClick={() => { setMode("signup"); setError(null); }}>Sign up</a></>) : (<>Already have an account? <a onClick={() => { setMode("login"); setError(null); }}>Sign in</a></>)}</div>
      </form>
    </div>
  );
}
