/**
 * JSON Schema for Flow definitions.
 * Registered with Monaco for inline validation + autocomplete.
 */

const triggerKinds = ["cron", "github-pr", "manual", "webhook"];
const sourceKinds = ["rss", "web-scrape", "github-merged-prs", "market-data"];
const executorKinds = ["claude-code", "claude-api"];
const sinkKinds = ["slack", "notion"];

export const flowJsonSchema = {
  $schema: "http://json-schema.org/draft-07/schema#",
  type: "object",
  required: ["id", "name", "nodes", "edges"],
  properties: {
    id: { type: "string", description: "Unique flow identifier (UUID)" },
    name: { type: "string", description: "Human-readable flow name" },
    description: { type: "string", description: "Flow description" },
    enabled: { type: "boolean", description: "Whether the flow is scheduled" },
    nodes: {
      type: "array",
      description: "DAG nodes",
      items: {
        type: "object",
        required: ["id", "node_type", "kind", "config", "position", "label"],
        properties: {
          id: { type: "string", description: "Node UUID" },
          node_type: {
            type: "string",
            enum: ["trigger", "source", "executor", "sink"],
            description: "Node category",
          },
          kind: {
            type: "string",
            description: "Specific node kind within its type",
            oneOf: [
              { enum: triggerKinds, description: "Trigger kinds" },
              { enum: sourceKinds, description: "Source kinds" },
              { enum: executorKinds, description: "Executor kinds" },
              { enum: sinkKinds, description: "Sink kinds" },
            ],
          },
          label: { type: "string", description: "Display label" },
          position: {
            type: "object",
            required: ["x", "y"],
            properties: {
              x: { type: "number" },
              y: { type: "number" },
            },
          },
          config: {
            type: "object",
            description: "Node-specific configuration",
            additionalProperties: true,
          },
        },
      },
    },
    edges: {
      type: "array",
      description: "Connections between nodes",
      items: {
        type: "object",
        required: ["id", "source", "target"],
        properties: {
          id: { type: "string", description: "Edge UUID" },
          source: { type: "string", description: "Source node ID" },
          target: { type: "string", description: "Target node ID" },
        },
      },
    },
    created_at: { type: "string" },
    updated_at: { type: "string" },
  },
};

/**
 * Register the flow schema with Monaco's JSON diagnostics.
 * Call once after Monaco is ready.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function registerFlowSchema(monaco: any) {
  monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
    validate: true,
    schemas: [
      {
        uri: "https://cthulu.dev/schemas/flow.json",
        fileMatch: ["*"],
        schema: flowJsonSchema,
      },
    ],
  });
}
