---
name: frontend-patterns
description: Use when adding UI features, search/filter components, modals, or working with React hooks (useMemo, useEffect, useDeferredValue) in cthulu-studio.
---

# Frontend Patterns (Cthulu Studio)

## When to Apply

- Adding new UI features (search bars, filters, modals) to Studio components
- Working with `useMemo`, `useEffect`, `useDeferredValue`, or `useCallback`
- Adding CSS styles to `cthulu-studio/src/styles.css`
- Modifying `TemplateGallery.tsx`, `TopBar.tsx`, or any component with user input

## Trace the User Flow Before Writing Code

Before adding a UI feature, trace the exact user journey:

1. What button/action triggers the feature?
2. What component renders at that point?
3. Add the feature to **that** component, not the nearest visible one.

**Example**: "Add search for templates" -> user clicks "Add New Flow" -> `TemplateGallery` modal opens -> search bar goes in `TemplateGallery.tsx`, NOT in `TopBar.tsx`.

## useDeferredValue for Input-Driven Filtering

When filtering a list on every keystroke, use `useDeferredValue` to keep the input responsive:

```tsx
const [searchQuery, setSearchQuery] = useState("");
const deferredSearch = useDeferredValue(searchQuery);

// Input reads searchQuery (immediate -- no typing lag)
<input value={searchQuery} onChange={(e) => setSearchQuery(e.target.value)} />

// Filtering reads deferredSearch (deferred -- React batches the expensive work)
const filtered = useMemo(() => {
  const q = deferredSearch.trim().toLowerCase();
  if (!q) return items;
  return items.filter((item) => item.title.toLowerCase().includes(q));
}, [items, deferredSearch]);
```

**Critical**: All UI that depends on the filtered results must read from `deferredSearch`, not `searchQuery`. This includes empty-state messages, result counts, and highlighted matches. Mixing immediate and deferred values causes visual mismatches during fast typing.

```tsx
// WRONG -- uses searchQuery (immediate) while filtered uses deferredSearch (deferred)
{filtered.length === 0 && searchQuery && `No results for "${searchQuery}"`}

// CORRECT -- both read from the same deferred value
{filtered.length === 0 && deferredSearch && `No results for "${deferredSearch}"`}
```

## Consolidate Related useEffects

Merge effects that share the same lifecycle scope (mount/unmount, same dependencies):

```tsx
// WRONG -- two effects, orphaned timer, split cleanup
useEffect(() => {
  setTimeout(() => inputRef.current?.focus(), 100);  // no cleanup!
}, []);
useEffect(() => {
  const handler = (e: KeyboardEvent) => { /* ... */ };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, [onClose, searchQuery]);

// CORRECT -- single effect, complete cleanup
useEffect(() => {
  const focusTimer = setTimeout(() => inputRef.current?.focus(), 100);
  const handler = (e: KeyboardEvent) => { /* ... */ };
  window.addEventListener("keydown", handler);
  return () => {
    clearTimeout(focusTimer);
    window.removeEventListener("keydown", handler);
  };
}, [onClose, searchQuery]);
```

## useMemo -- Clear Two-Step Filtering

When filtering has multiple stages (e.g., category then text search), structure the `useMemo` with clear steps and early returns:

```tsx
const filtered = useMemo(() => {
  // Step 1: filter by category
  const byCategory =
    activeCategory === "all"
      ? templates
      : templates.filter((t) => t.category === activeCategory);

  // Step 2: filter by search query
  const q = deferredSearch.trim().toLowerCase();
  if (!q) return byCategory;

  return byCategory.filter((t) =>
    t.title.toLowerCase().includes(q) ||
    t.description.toLowerCase().includes(q) ||
    t.tags.some((tag) => tag.toLowerCase().includes(q))
  );
}, [templates, activeCategory, deferredSearch]);
```

## Escape Key UX Pattern for Modals with Search

When a modal has a search input, Escape should clear the search first, then close the modal on second press:

```tsx
if (e.key === "Escape") {
  if (searchQuery && document.activeElement === searchRef.current) {
    setSearchQuery("");    // First Escape: clear search
  } else {
    onClose();             // Second Escape: close modal
  }
}
```

## CSS Naming Conventions

- Component-scoped prefix: `.tg-` (TemplateGallery), `.topbar-` (TopBar), `.vm-terminal-` (VmTerminal)
- State modifiers: `.tg-search--focused`, `.tg-card.hovered`
- Always use CSS variables: `var(--bg)`, `var(--border)`, `var(--accent)`, `var(--text)`, `var(--text-secondary)`, `var(--bg-secondary)`, `var(--bg-tertiary)`
- Font stack for code areas: `"SF Mono", "Fira Code", "Cascadia Code", monospace`
- All styles live in `cthulu-studio/src/styles.css` -- no CSS modules or inline styles (except trivial one-offs)

## Search Bar Component Checklist

When adding a search bar to any component:

1. State: `useState` for query + `useDeferredValue` for filtering
2. Ref: `useRef<HTMLInputElement>` for auto-focus and Escape handling
3. Auto-focus: `setTimeout(() => ref.current?.focus(), 100)` with `clearTimeout` cleanup
4. Icon: SVG magnifying glass (14-16px, `stroke="currentColor"`)
5. Clear button: appears when query is non-empty, resets query and re-focuses input
6. Keyboard: Escape clears query first, then triggers parent close
7. Empty state: uses deferred value, not immediate query
8. Styles: themed with CSS variables, focus ring with `box-shadow: 0 0 0 2px color-mix(...)`

## File Locations

| What | Where |
|------|-------|
| All CSS styles | `cthulu-studio/src/styles.css` |
| Template gallery (search, categories, cards) | `cthulu-studio/src/components/TemplateGallery.tsx` |
| Top navigation bar | `cthulu-studio/src/components/TopBar.tsx` |
| UI primitives (Button, Select, Input) | `cthulu-studio/src/components/ui/` |
| Type definitions | `cthulu-studio/src/types/flow.ts` |
