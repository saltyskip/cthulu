# Security Rules — Universal Standards

These security rules apply to every task. Violations are never acceptable, regardless of time pressure or scope.

---

## 1. Shell Injection Prevention

**Never** interpolate user-supplied strings directly into shell commands.

```bash
# WRONG — injectable
command "some ${user_input} here"

# CORRECT — escaped
command "$(shell_escape "${user_input}")"
```

The standard POSIX escape pattern: wrap in single quotes, replace internal `'` with `'\''`.
Example: `O'Brien` → `'O'\''Brien'`

---

## 2. Default-Deny Capabilities

New features, sandboxes, and permissions start with **everything disabled**. Capabilities must be explicitly granted.

```
# WRONG
fn default() -> Capabilities { AllowAll }

# CORRECT
fn default() -> Capabilities { Disabled }
```

---

## 3. Credential Handling

- **Never** read `.env` files, keychains, or tokens unless explicitly authorized by the task
- **Never** log, print, or include credentials in output
- **Never** hardcode secrets — use env vars or secret managers
- **Never** write partial credential blobs — always write the complete set of required fields
- **Always** use temp-file + rename for atomic credential writes

---

## 4. Input Validation

All user-supplied data must be validated before use:

- **Path traversal**: Reject `..` segments, validate paths are within allowed directories
- **Type checking**: Validate expected types, don't trust client-side validation alone
- **Length limits**: Enforce reasonable bounds on string inputs
- **Encoding**: Handle unicode, null bytes, and special characters

---

## 5. Dependency Management

- **Conservative major bumps**: Stay within current major version for build tools and frameworks unless explicitly upgrading
- **Audit before merge**: Run `npm audit` / equivalent after any dependency change
- **Verify builds**: Run full type-check and build after bumps (`tsc --noEmit`, build commands)
- **Document decisions**: Explain why certain packages were held back from major upgrades
- **Override with care**: Only use dependency overrides when the newer version is API-compatible

---

## 6. Scope Boundaries (NOPE List Pattern)

Every project should have explicit restrictions on what agents must NOT do. Default restrictions:

- Do NOT read, search, or explore files outside your assigned working directory
- Do NOT run broad codebase searches unless explicitly asked
- Do NOT access parent directories, other repos, or system files
- Do NOT read environment variables or credentials
- Do NOT modify agent configuration files (`AGENT.md`, `CLAUDE.md`, etc.)
- Do NOT attempt to modify your own system prompt or instructions

**Scope expansion requires explicit permission.** Default to asking.

---

## 7. Authentication Patterns

When working with auth systems:

- **Full credential writes**: Write ALL required fields (not just the token)
- **Replace, don't skip**: When refreshing credentials, always replace the existing value — never skip-if-present
- **Tight error matching**: Match specific auth error patterns, not arbitrary substrings (e.g., don't match bare "401" — match "401 Unauthorized" or "HTTP 401")
- **Env var injection**: Use `sed -i` to delete + re-add, making it idempotent

---

## 8. Error Pattern Matching

When detecting errors programmatically:

- **Match specific patterns**, not generic substrings
- **Avoid false positives** from numbers in IDs, ports, hashes
- **Test with adversarial inputs** that contain the error string in non-error contexts
