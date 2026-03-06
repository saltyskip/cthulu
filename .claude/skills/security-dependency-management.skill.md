---
name: security-dependency-management
description: Use when running npm audit, bumping package versions, fixing vulnerabilities, or resolving workspace dependency hoisting issues.
---

# Security and Dependency Management

## When to Apply

- Running `npm audit` or `npm audit fix`
- Bumping package versions across the monorepo
- Resolving `ERR_MODULE_NOT_FOUND` or hoisting errors
- Creating security-focused PRs
- Modifying any `package.json` or `package-lock.json`

## npm Audit Workflow

```bash
# 1. Check current vulnerabilities
npm audit

# 2. Fix what can be auto-fixed (use --legacy-peer-deps in workspaces)
npm audit fix --legacy-peer-deps

# 3. Check what remains
npm audit

# 4. For remaining -- check if fix is available upstream
# If "No fix available" -- document it, don't force-break things
```

**Important**: Some vulnerabilities have no upstream fix (e.g., transitive deps deep in `@nx/webpack`). Document these in the PR but do not force-upgrade major versions just to resolve them.

## Bumping Packages -- Safe vs Aggressive

Use `npm-check-updates` (`ncu`) to find latest versions:

```bash
# Check what's outdated (dry run)
npx npm-check-updates --packageFile cthulu-studio/package.json

# Bump everything to latest
npx npm-check-updates --packageFile cthulu-studio/package.json -u

# Bump within minor only (safe for build tools)
npx npm-check-updates --packageFile package.json -u --target minor

# Bump everything EXCEPT a specific package
npx npm-check-updates --packageFile cthulu-site/package.json -u --reject next

# Bump only a specific package within minor
npx npm-check-updates --packageFile cthulu-site/package.json -u --filter next --target minor
```

### Major Version Bump Policy

| Package | Policy | Reason |
|---------|--------|--------|
| Nx (`nx`, `@nx/*`) | Minor only (`--target minor`) | Major bumps change workspace config, plugin APIs, and migration scripts |
| Next.js (`next`) | Minor only (`--target minor`) | Major bumps change routing, rendering, and config APIs |
| React / React DOM | Latest OK | React 19 is stable, minor bumps are safe |
| TypeScript | Latest OK | Backwards compatible within 5.x |
| Vite | Latest OK | Backwards compatible within major |
| Everything else | Latest OK | Patch and minor bumps are generally safe |

## npm Workspace Hoisting

In this monorepo, npm hoists shared dependencies to the root `node_modules/`. This can cause issues:

**Problem**: Package A (hoisted to root) depends on package B, but B only exists in a workspace's `node_modules/`. Node resolution fails because A resolves from root.

**Fix**: Add B to the root `package.json` so it gets hoisted alongside A.

**Real example**: `@tailwindcss/vite` (hoisted to root) imported `vite`, but `vite` was only in `cthulu-studio/node_modules/`. Fix: add `"vite": "^7.3.1"` to root `devDependencies`.

**Diagnostic**:
```bash
# Check if a package exists at root
ls node_modules/vite/package.json

# Check if it exists in a workspace
ls cthulu-studio/node_modules/vite/package.json

# If root is missing but workspace has it -- add to root package.json
```

## Verification After Bumps

Always run these after any dependency change:

```bash
# 1. Install
npm install --legacy-peer-deps

# 2. TypeScript check for all projects
npx tsc --noEmit  # run in cthulu-studio/
npx tsc --noEmit  # run in cthulu-site/

# 3. Build check
npx vite build     # run in cthulu-studio/ (faster than full nx build)

# 4. Final audit
npm audit
```

## Overrides for Transitive Vulnerabilities

The root `package.json` has an `overrides` block for forcing transitive dependency versions:

```json
{
  "overrides": {
    "minimatch": "^10.2.1",
    "koa": "^3.1.1",
    "webpack": "^5.105.0"
  }
}
```

Use this when a vulnerable package is a transitive dependency that can't be fixed via `npm audit fix`. Only override when you've verified the newer version is API-compatible.

## PR Template for Security Updates

When creating a security PR, include:

1. **Audit before/after table** -- which CVEs were fixed, which remain
2. **Package bump table** -- old version -> new version per package per workspace
3. **Major version decisions** -- why certain packages were held back
4. **Remaining vulnerabilities** -- with "no fix available" notation
5. **Verification** -- `tsc --noEmit` and build pass confirmation

## File Locations

| What | Where |
|------|-------|
| Root workspace config | `package.json` (workspaces, overrides, Nx deps) |
| Studio dependencies | `cthulu-studio/package.json` |
| Site dependencies | `cthulu-site/package.json` |
| Lock file | `package-lock.json` (always commit after changes) |
| Nx workspace config | `nx.json` |
