# Cthulu Project Guidelines

## React Patterns

- **Avoid `useEffect` for derived state sync.** Use `useMemo` for computed values, callback forms of state setters to access current state, and event handlers for side effects. When `useEffect` is truly needed (e.g., syncing an external prop into internal state), minimize the dependency array — never depend on state that the effect itself modifies.

## Cthulu Studio (React Flow)

- **Never replace React Flow nodes wholesale.** Calling `setNodes(newArray)` with entirely new node objects destroys React Flow's internal measurements (`measured`, `internals`, `handleBounds`), causing edge renderers to crash. Always merge changes into existing nodes via spread (`{ ...existingNode, data: newData }`).
