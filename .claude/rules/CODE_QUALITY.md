# Code Quality Rules — Universal Standards

These rules apply across all languages and frameworks. Project-specific patterns live in the project's skill files.

---

## 1. State Management

- **No `useEffect` for derived state** — use `useMemo` for computed values, callback forms of state setters, and event handlers for side effects
- **Only use `useEffect`** when truly needed: syncing with external systems, subscriptions, or DOM manipulation
- **Never depend on state that the effect itself modifies** — this creates infinite loops
- **Immutable updates** — always create new objects/arrays, never mutate in place

```tsx
// WRONG — useEffect for derived state
useEffect(() => { setFiltered(items.filter(predicate)); }, [items]);

// CORRECT — useMemo for computed values
const filtered = useMemo(() => items.filter(predicate), [items]);
```

---

## 2. Effect Consolidation

Merge effects that share the same lifecycle scope:

- **One effect per concern**, not one effect per side-effect
- **Complete cleanup** — every timer, listener, and subscription gets cleaned up
- **No orphaned timers** — `setTimeout` without `clearTimeout` in cleanup is a memory leak

```tsx
// WRONG — split effects, orphaned timer
useEffect(() => { setTimeout(() => ref.current?.focus(), 100); }, []);
useEffect(() => {
  const handler = (e) => { /* ... */ };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, []);

// CORRECT — merged, complete cleanup
useEffect(() => {
  const timer = setTimeout(() => ref.current?.focus(), 100);
  const handler = (e) => { /* ... */ };
  window.addEventListener("keydown", handler);
  return () => { clearTimeout(timer); window.removeEventListener("keydown", handler); };
}, []);
```

---

## 3. Deferred Values Consistency

When using `useDeferredValue` for performance, **all dependent UI must read from the same deferred value**:

```tsx
// WRONG — mixing immediate and deferred
{filtered.length === 0 && searchQuery && `No results for "${searchQuery}"`}

// CORRECT — both use deferred
{filtered.length === 0 && deferredSearch && `No results for "${deferredSearch}"`}
```

---

## 4. Async Mutex Discipline

- **Use async-compatible locks** when the lock is held across `.await` points
- **Use standard locks** only when held briefly with no `.await` inside
- **Single lock pattern** — acquire once, drain all data into locals, drop the lock, then process

```rust
// WRONG — double lock with race window
let data = pool.lock().await.get(&key).clone();
drop(pool);  // another task can modify pool here!
let more = pool.lock().await.get(&key);  // might be gone

// CORRECT — single lock, drain into locals
let (data, more) = {
    let pool = pool.lock().await;
    (pool.get(&key).clone(), pool.get_extra(&key))
};
// Process data outside the lock
```

---

## 5. Error Handling Patterns

Choose the right strategy per failure type:

| Failure Type | Strategy | Example |
|-------------|----------|---------|
| **Recoverable, non-critical** | Log + continue (best-effort) | Network source fetch |
| **Recoverable, critical** | Retry with backoff | Auth token refresh |
| **Unrecoverable** | Fail fast with clear error | Missing required config |
| **Partial success** | Log failures, continue with successes | Multi-target delivery |

---

## 6. Comments

- Comment **why**, never **what** — the code should be self-explanatory
- `// increment counter` above `counter++` adds negative value
- Good comments explain business logic, non-obvious constraints, or workarounds
- Remove stale comments that no longer match the code

---

## 7. Version Bump Policy

- **Patch/minor bumps**: Generally safe, apply freely
- **Major bumps**: Require explicit decision — check for breaking changes, migration guides
- **Build tool major bumps** (bundlers, monorepo tools, frameworks): Stay conservative, bump within current major
- **Always verify** with type-check and build commands after any bump

---

## 8. UI Feature Placement

Before adding a UI feature, trace the exact user journey:

1. What button/action triggers the feature?
2. What component renders at that point?
3. Add the feature to **that** component, not the nearest visible one

---

## 9. CSS Discipline

- Use CSS variables for theming — never hardcode colors
- Component-scoped class prefixes (e.g., `.modal-`, `.sidebar-`)
- All styles in a central stylesheet or CSS modules — no scattered inline styles
- Font stacks for code areas should use monospace families

---

## 10. Process Handle Safety

Types that represent OS resources (process handles, file descriptors, stream receivers) are typically NOT cloneable. Never derive `Clone` on structs containing them. Use `Arc<Mutex<..>>` for shared access.
