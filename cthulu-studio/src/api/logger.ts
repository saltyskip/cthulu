export type LogLevel = "info" | "warn" | "error" | "http";

export interface LogEntry {
  id: number;
  timestamp: Date;
  level: LogLevel;
  message: string;
  detail?: string;
}

type Listener = () => void;

const MAX_ENTRIES = 500;

let entries: LogEntry[] = [];
let nextId = 1;
let listeners: Listener[] = [];

function emit() {
  for (const fn of listeners) fn();
}

export function log(level: LogLevel, message: string, detail?: string) {
  entries.push({ id: nextId++, timestamp: new Date(), level, message, detail });
  if (entries.length > MAX_ENTRIES) {
    entries = entries.slice(-MAX_ENTRIES);
  }
  emit();
}

export function getEntries(): LogEntry[] {
  return entries;
}

export function clearEntries() {
  entries = [];
  nextId = 1;
  emit();
}

export function subscribe(fn: Listener): () => void {
  listeners.push(fn);
  return () => {
    listeners = listeners.filter((l) => l !== fn);
  };
}
