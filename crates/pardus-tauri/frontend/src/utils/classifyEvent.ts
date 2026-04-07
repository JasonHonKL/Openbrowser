/**
 * Shared event classification utilities.
 * Extracted from ActionLog.tsx for reuse by GlobalActionStream.
 */

export interface ActionEntry {
  id: string;
  timestamp: number;
  type: "navigate" | "action_start" | "action_complete" | "action_fail";
  summary: string;
  detail?: string;
}

export function classifyEvent(event: {
  method: string;
  params: Record<string, unknown>;
  timestamp: number;
}): ActionEntry | null {
  const { method, params, timestamp } = event;

  if (method === "Page.frameNavigated") {
    const frame = (params as { frame?: { url?: string } })?.frame;
    const url = frame?.url ?? "unknown";
    return {
      id: `nav-${timestamp}`,
      timestamp,
      type: "navigate",
      summary: "Navigate",
      detail: url,
    };
  }

  if (method === "Pardus.actionStarted") {
    const p = params as { action?: string; target?: { selector?: string } };
    const action = p?.action ?? "unknown";
    const selector = p?.target?.selector ?? "";
    return {
      id: `act-s-${timestamp}`,
      timestamp,
      type: "action_start",
      summary: action.charAt(0).toUpperCase() + action.slice(1),
      detail: selector,
    };
  }

  if (method === "Pardus.actionCompleted") {
    const p = params as { action?: string; result?: { note?: string } };
    const action = p?.action ?? "unknown";
    const note = p?.result?.note ?? "";
    return {
      id: `act-c-${timestamp}`,
      timestamp,
      type: "action_complete",
      summary: `${action} done`,
      detail: note || undefined,
    };
  }

  if (method === "Pardus.actionFailed") {
    const p = params as { action?: string; result?: { error?: string } };
    const action = p?.action ?? "unknown";
    const error = p?.result?.error ?? "unknown error";
    return {
      id: `act-f-${timestamp}`,
      timestamp,
      type: "action_fail",
      summary: `${action} failed`,
      detail: error,
    };
  }

  return null;
}

export function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString("en-US", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export const TYPE_ICONS: Record<ActionEntry["type"], string> = {
  navigate: "\u2192",
  action_start: "\u25B6",
  action_complete: "\u2713",
  action_fail: "\u2717",
};

export const TYPE_COLORS: Record<ActionEntry["type"], string> = {
  navigate: "var(--accent)",
  action_start: "var(--cyan)",
  action_complete: "var(--green)",
  action_fail: "var(--red)",
};

/** Color palette for instances in the global action stream */
export const INSTANCE_COLORS = [
  "#0a84ff", // blue
  "#30d158", // green
  "#ff9f0a", // orange
  "#bf5af2", // purple
  "#ff453a", // red
  "#64d2ff", // cyan
  "#ffd60a", // yellow
  "#ff375f", // pink
];

export function getInstanceColor(index: number): string {
  return INSTANCE_COLORS[index % INSTANCE_COLORS.length];
}
