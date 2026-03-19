import { useState, useEffect } from "react";
import { setAuthTokenGetter, getServerUrl } from "../api/client";

interface AuthGateProps {
  children: React.ReactNode;
}

/**
 * Self-hosted auth gate. Shows login/signup when no token,
 * renders children when authenticated.
 * When VITE_AUTH_ENABLED is not "true", renders children directly (dev mode).
 */
export default function AuthGate({ children }: AuthGateProps) {
  const authEnabled = import.meta.env.VITE_AUTH_ENABLED === "true";
  const [token, setToken] = useState<string | null>(
    () => localStorage.getItem("cthulu_auth_token"),
  );

  // Wire up auth token getter as a side effect, not during render
  useEffect(() => {
    if (token) {
      setAuthTokenGetter(() => Promise.resolve(token));
    }
    return () => setAuthTokenGetter(null);
  }, [token]);

  if (!authEnabled) return <>{children}</>;
  if (token) return <>{children}</>;

  return <AuthForm onSuccess={(t) => {
    localStorage.setItem("cthulu_auth_token", t);
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
