/**
 * Cthulu Cloud Agent — A2A Server.
 *
 * Starts a Google ADK A2A server that exposes the agent on the configured port.
 * The Cthulu backend sends tasks to this server via the A2A JSON-RPC protocol.
 *
 * Environment variables:
 *   PORT           — HTTP port (default: 3000)
 *   ANTHROPIC_API_KEY — Required for Claude model access
 *   VM_ID          — VM identifier (set by provisioning script)
 *   AGENT_NAME     — Agent name override (default: cthulu-cloud-agent)
 */
import { A2AServer } from '@google/adk';
import { rootAgent } from './agent.js';

const port = parseInt(process.env.PORT || '3000', 10);

const server = new A2AServer({
  agent: rootAgent,
  card: {
    name: rootAgent.name,
    description: rootAgent.description,
    url: `http://0.0.0.0:${port}`,
    capabilities: {
      streaming: true,
      pushNotifications: false,
    },
    skills: [
      {
        id: 'execute-command',
        name: 'Execute Shell Commands',
        description: 'Run arbitrary shell commands in the VM',
      },
      {
        id: 'file-operations',
        name: 'File Operations',
        description: 'Read, write, and list files in the VM',
      },
      {
        id: 'code-execution',
        name: 'Code Execution',
        description: 'Write and execute code in any language installed in the VM',
      },
    ],
  },
});

server.start({ port });
console.log(`A2A agent server listening on http://0.0.0.0:${port}`);
console.log(`Agent: ${rootAgent.name}`);
console.log(`Model: claude-sonnet-4-20250514`);
