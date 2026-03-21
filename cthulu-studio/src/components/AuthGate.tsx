import { useState, useCallback, createContext, useContext } from "react";
import { setAuthTokenGetter, getServerUrl } from "../api/client";

// ── Auth context so any component can call logout ────────────

interface AuthContextValue {
  logout: () => void;
  userEmail: string | null;
}

const AuthContext = createContext<AuthContextValue>({
  logout: () => {},
  userEmail: null,
});

/** Hook for any component to access logout + user info. */
export function useAuth() {
  return useContext(AuthContext);
}

interface AuthGateProps {
  children: React.ReactNode;
}

/** Decode base64url to string (handles missing padding). */
function b64urlDecode(str: string): string {
  const padded = str.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (str.length % 4)) % 4);
  return atob(padded);
}

/** Parse JWT payload and return claims, or null if unparseable. */
function parseToken(token: string): { email?: string; exp?: number } | null {
  try {
    const payload = token.split(".")[1];
    return JSON.parse(b64urlDecode(payload));
  } catch {
    return null;
  }
}

/** Check if token is valid: parseable, has email, not expired. */
function isTokenValid(token: string): boolean {
  const claims = parseToken(token);
  if (!claims?.email) return false;
  if (claims.exp && claims.exp * 1000 < Date.now()) return false;
  return true;
}

/**
 * Self-hosted auth gate. Shows login/signup when no token,
 * renders children when authenticated.
 * When VITE_AUTH_ENABLED is not "true", renders children directly (dev mode).
 */
export default function AuthGate({ children }: AuthGateProps) {
  const authEnabled = import.meta.env.VITE_AUTH_ENABLED === "true";

  // Initialize token AND token getter synchronously to avoid race condition
  // where children's effects fire API calls before the getter is wired up.
  const [token, setToken] = useState<string | null>(() => {
    const t = localStorage.getItem("cthulu_auth_token");
    if (t && isTokenValid(t)) {
      setAuthTokenGetter(() => Promise.resolve(t));
      return t;
    }
    // Clear invalid/expired token
    if (t) localStorage.removeItem("cthulu_auth_token");
    setAuthTokenGetter(null);
    return null;
  });

  const logout = useCallback(() => {
    localStorage.removeItem("cthulu_auth_token");
    setAuthTokenGetter(null);
    setToken(null);
  }, []);

  if (!authEnabled) return <>{children}</>;

  if (token) {
    const claims = parseToken(token);
    return (
      <AuthContext.Provider value={{ logout, userEmail: claims?.email ?? null }}>
        {children}
      </AuthContext.Provider>
    );
  }

  return <AuthForm onSuccess={(t) => {
    localStorage.setItem("cthulu_auth_token", t);
    setAuthTokenGetter(() => Promise.resolve(t));
    setToken(t);
  }} />;
}

function AuthForm({ onSuccess }: { onSuccess: (token: string) => void }) {
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      const endpoint = mode === "signup" ? "/api/auth/signup" : "/api/auth/login";
      const res = await fetch(`${getServerUrl()}${endpoint}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password }),
      });

      const data = await res.json();

      if (!res.ok) {
        setError(data.error || `Failed (${res.status})`);
        return;
      }

      onSuccess(data.token);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Network error");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="auth-container">
      <div className="auth-card">
        <h1 className="auth-title">Cthulu</h1>
        <p className="auth-subtitle">
          {mode === "login" ? "Sign in to your account" : "Create a new account"}
        </p>

        <form onSubmit={handleSubmit} className="auth-form">
          <input
            type="email"
            placeholder="Email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="auth-input"
            required
            autoFocus
          />
          <input
            type="password"
            placeholder="Password (8+ characters)"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="auth-input"
            required
            minLength={8}
          />

          {error && <p className="auth-error">{error}</p>}

          <button type="submit" className="auth-button" disabled={loading}>
            {loading ? "..." : mode === "login" ? "Sign In" : "Sign Up"}
          </button>
        </form>

        <button
          className="auth-toggle"
          onClick={() => { setMode(mode === "login" ? "signup" : "login"); setError(""); }}
        >
          {mode === "login"
            ? "Don't have an account? Sign up"
            : "Already have an account? Sign in"}
        </button>
      </div>
    </div>
  );
}
