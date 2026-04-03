import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ChallengeInfo, LogEntry } from "./types";

type LogCallback = (entry: LogEntry) => void;

export function onChallengeDetected(callback: (info: ChallengeInfo) => void): Promise<UnlistenFn> {
  return listen<ChallengeInfo>("challenge-detected", (event) => {
    callback(event.payload);
  });
}

export function onChallengeSolved(callback: (info: ChallengeInfo) => void): Promise<UnlistenFn> {
  return listen<ChallengeInfo>("challenge-solved", (event) => {
    callback(event.payload);
  });
}

export function onChallengeFailed(callback: (url: string, reason: string) => void): Promise<UnlistenFn> {
  return listen<{ challenge_url: string; reason: string }>("challenge-failed", (event) => {
    callback(event.payload.challenge_url, event.payload.reason);
  });
}

export function createLogger(container: HTMLElement): LogCallback {
  return (entry: LogEntry) => {
    const div = document.createElement("div");
    div.className = `log-entry ${entry.level}`;
    div.textContent = `[${entry.timestamp}] ${entry.message}`;
    container.appendChild(div);
    container.scrollTop = container.scrollHeight;
  };
}

export function log(level: LogEntry["level"], message: string): LogEntry {
  return {
    level,
    message,
    timestamp: new Date().toISOString().slice(11, 19),
  };
}
