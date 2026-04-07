import { useEffect, useRef, useCallback, useState } from "react";
import type { ServerEvent } from "../api/client";

interface UseEventsReturn {
  events: ServerEvent[];
  connected: boolean;
}

export function useEvents(): UseEventsReturn {
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout>>();

  const connect = useCallback(() => {
    const protocol = location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(`${protocol}//${location.host}/ws`);

    ws.onopen = () => setConnected(true);
    ws.onclose = () => {
      setConnected(false);
      reconnectTimerRef.current = setTimeout(connect, 3000);
    };
    ws.onmessage = (e) => {
      try {
        const event: ServerEvent = JSON.parse(e.data);
        setEvents((prev) => [...prev.slice(-99), event]);
      } catch {
        // ignore malformed messages
      }
    };

    wsRef.current = ws;
  }, []);

  useEffect(() => {
    connect();
    return () => {
      if (reconnectTimerRef.current) clearTimeout(reconnectTimerRef.current);
      wsRef.current?.close();
    };
  }, [connect]);

  return { events, connected };
}
