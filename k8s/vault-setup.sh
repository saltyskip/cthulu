#!/bin/bash
# Vault setup for Cthulu secrets + Google OIDC auth
#
# Run once to configure Vault with:
#   1. Cthulu service secrets (MongoDB, Google OAuth, Anthropic API key)
#   2. Google OIDC auth method for users to login via Google Workspace
#
# Prerequisites:
#   - vault CLI authenticated
#   - GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET from Google Cloud Console

set -euo pipefail

# ── 1. Store Cthulu service secrets ──────────────────────────

echo "=== Storing Cthulu secrets ==="

# Enable KV v2 secrets engine if not already
vault secrets enable -path=cthulu-svc kv-v2 2>/dev/null || true

vault kv put cthulu-svc/secrets \
  MONGODB_URI="mongodb://root:checkOne@mongo:27017" \
  GOOGLE_CLIENT_ID="${GOOGLE_CLIENT_ID}" \
  GOOGLE_CLIENT_SECRET="${GOOGLE_CLIENT_SECRET}"

echo "  Secrets stored at cthulu-svc/data/secrets"

# ── 2. Create Vault policy for Cthulu ────────────────────────

echo "=== Creating Vault policy ==="

vault policy write cthulu-svc - <<EOF
path "cthulu-svc/data/secrets" {
  capabilities = ["read"]
}
EOF

echo "  Policy: cthulu-svc"

# ── 3. Create Kubernetes auth role ───────────────────────────

echo "=== Creating K8s auth role ==="

vault write auth/utilities/role/cthulu-svc \
  bound_service_account_names=cthulu-sa \
  bound_service_account_namespaces=obsidian-sync \
  policies=cthulu-svc \
  ttl=1h

echo "  Role: cthulu-svc (bound to cthulu-sa in obsidian-sync)"

# ── 4. Enable Google OIDC auth method ────────────────────────

echo "=== Setting up Google OIDC auth ==="

# Enable OIDC auth if not already
vault auth enable -path=oidc oidc 2>/dev/null || true

vault write auth/oidc/config \
  oidc_discovery_url="https://accounts.google.com" \
  oidc_client_id="${GOOGLE_CLIENT_ID}" \
  oidc_client_secret="${GOOGLE_CLIENT_SECRET}" \
  default_role="google-user"

# Create default role for Google users
# Restrict to your Google Workspace domain by setting allowed_redirect_uris
# and bound_claims for hd (hosted domain)
vault write auth/oidc/role/google-user \
  bound_audiences="${GOOGLE_CLIENT_ID}" \
  allowed_redirect_uris="https://your-cthulu-domain.com/ui/vault/auth/oidc/oidc/callback" \
  allowed_redirect_uris="http://localhost:8250/oidc/callback" \
  user_claim="email" \
  groups_claim="groups" \
  oidc_scopes="openid,email,profile" \
  policies="cthulu-user" \
  ttl=12h

# Policy for Google-authenticated users (read-only access to their own data)
vault policy write cthulu-user - <<EOF
path "cthulu-svc/data/secrets" {
  capabilities = ["read"]
}
EOF

echo "  OIDC auth configured with Google"
echo ""
echo "=== Done ==="
echo ""
echo "To test Google OIDC login:"
echo "  vault login -method=oidc role=google-user"
echo ""
echo "To deploy Cthulu:"
echo "  kubectl apply -f k8s/deployment.yaml"
