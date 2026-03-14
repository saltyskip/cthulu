/**
 * Cthulu Cloud Agent — ADK agent definition.
 *
 * This agent runs inside a Firecracker microVM and accepts tasks from
 * the Cthulu backend via the A2A protocol.  It uses Claude as its LLM
 * and has tools for executing shell commands and file operations within
 * the VM.
 */
import { LlmAgent, FunctionTool } from '@google/adk';
import { z } from 'zod';
import { execSync } from 'child_process';
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'fs';

// ── Tools ──────────────────────────────────────────────────────────────────

const executeCommand = new FunctionTool({
  name: 'execute_command',
  description: 'Execute a shell command in the VM and return stdout/stderr. Use for running code, installing packages, git operations, etc.',
  parameters: z.object({
    command: z.string().describe('The shell command to execute'),
    working_dir: z.string().optional().describe('Working directory (default: /home/agent/workspace)'),
    timeout_ms: z.number().optional().describe('Timeout in milliseconds (default: 60000)'),
  }),
  execute: ({ command, working_dir, timeout_ms }) => {
    const cwd = working_dir || '/home/agent/workspace';
    const timeout = timeout_ms || 60000;
    try {
      const output = execSync(command, {
        cwd,
        timeout,
        encoding: 'utf-8',
        maxBuffer: 10 * 1024 * 1024, // 10MB
        stdio: ['pipe', 'pipe', 'pipe'],
      });
      return { status: 'success', output: output.trim() };
    } catch (err) {
      return {
        status: 'error',
        exit_code: err.status || 1,
        stdout: (err.stdout || '').trim(),
        stderr: (err.stderr || '').trim(),
      };
    }
  },
});

const readFile = new FunctionTool({
  name: 'read_file',
  description: 'Read the contents of a file in the VM.',
  parameters: z.object({
    path: z.string().describe('Absolute path to the file'),
  }),
  execute: ({ path }) => {
    try {
      if (!existsSync(path)) {
        return { status: 'error', error: `File not found: ${path}` };
      }
      const content = readFileSync(path, 'utf-8');
      return { status: 'success', content };
    } catch (err) {
      return { status: 'error', error: err.message };
    }
  },
});

const writeFile = new FunctionTool({
  name: 'write_file',
  description: 'Write content to a file in the VM. Creates directories as needed.',
  parameters: z.object({
    path: z.string().describe('Absolute path to the file'),
    content: z.string().describe('Content to write'),
  }),
  execute: ({ path, content }) => {
    try {
      const dir = path.substring(0, path.lastIndexOf('/'));
      if (dir) execSync(`mkdir -p "${dir}"`);
      writeFileSync(path, content, 'utf-8');
      return { status: 'success', path };
    } catch (err) {
      return { status: 'error', error: err.message };
    }
  },
});

const listDirectory = new FunctionTool({
  name: 'list_directory',
  description: 'List files and directories at the given path.',
  parameters: z.object({
    path: z.string().describe('Absolute path to the directory'),
  }),
  execute: ({ path }) => {
    try {
      if (!existsSync(path)) {
        return { status: 'error', error: `Directory not found: ${path}` };
      }
      const entries = readdirSync(path, { withFileTypes: true }).map((e) => ({
        name: e.name,
        type: e.isDirectory() ? 'directory' : 'file',
      }));
      return { status: 'success', entries };
    } catch (err) {
      return { status: 'error', error: err.message };
    }
  },
});

// ── Agent ──────────────────────────────────────────────────────────────────

const agentName = process.env.AGENT_NAME || 'cthulu-cloud-agent';
const vmId = process.env.VM_ID || 'unknown';

export const rootAgent = new LlmAgent({
  name: `${agentName}-vm${vmId}`,
  model: 'claude-sonnet-4-20250514',
  description: `Cloud executor agent running on VM ${vmId}. Executes tasks in an isolated Firecracker microVM with full shell access.`,
  instruction: `You are a cloud executor agent running inside an isolated virtual machine.
You have full root access to this VM and can execute any shell commands, read/write files, and install packages.

Your workspace is at /home/agent/workspace. Use it for any file operations.

When given a task:
1. Break it down into steps
2. Execute each step using the available tools
3. Return a clear, structured result

You can install any packages you need using apt-get or npm/pip.
Always report both successes and failures clearly.`,
  tools: [executeCommand, readFile, writeFile, listDirectory],
});
