# Workflow Context

<!-- This file is auto-generated at runtime by the Cthulu backend when a user
     opens an executor node for the first time. The content below is replaced
     with live pipeline data for the specific flow and executor node.

     Team members: use this file as a reference for what gets injected.
     Do not hardcode flow-specific content here. -->

## Flow: [flow-name]

[Flow description]

## Your Position in the Pipeline

```
  Trigger: [trigger kind] -- [schedule or event]
    -> Source: [source label] ([source kind]) -- [key config]
    -> Source: [source label] ([source kind]) -- [key config]
    -> **YOU: [executor label] ([executor kind])**
    -> Sink: [sink label] ([sink kind]) -- [key config]
```

## Upstream Nodes (feeding data into you)

| Node | Kind | Config |
|------|------|--------|
| [source label] | [kind] | [key: value, key: value] |

## Downstream Nodes (receiving your output)

| Node | Kind | Config |
|------|------|--------|
| [sink label] | [kind] | [key: value] |

## All Executors in This Flow

- **E01: [executor label] (this node)** -- prompt: [prompt path or inline]

## Your Configuration

- **Label**: [executor label]
- **Prompt path**: [prompt file path, or "inline"]
- **Node ID**: [uuid]
