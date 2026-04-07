import { useState, useRef, useEffect, useCallback } from "react";
import { useAgent } from "../context/AgentContext";
import type { ChatMessage, ToolCallInfo } from "../types";

function ToolCallBadge({ tc }: { tc: ToolCallInfo }) {
  const success = tc.result?.success;
  const duration = tc.result?.duration_ms ?? 0;
  return (
    <div
      className={`chat-tool-call ${success ? "chat-tool-success" : "chat-tool-pending"}`}
    >
      <span className="chat-tool-icon">{success ? "\u2713" : "\u25B6"}</span>
      <span className="chat-tool-name">{tc.name}</span>
      {duration > 0 && (
        <span className="chat-tool-duration">{duration}ms</span>
      )}
    </div>
  );
}

function MessageBubble({
  message,
}: {
  message: ChatMessage;
}) {
  const isUser = message.role === "user";
  const isTool = message.role === "tool";
  const isAssistant = message.role === "assistant";

  return (
    <div className={`chat-msg chat-msg-${message.role}`}>
      {!isUser && (
        <span className={`chat-msg-role chat-role-${message.role}`}>
          {isTool ? "tool" : "agent"}
        </span>
      )}
      <div className="chat-msg-body">
        {message.content && (
          <div
            className={`chat-msg-content ${message.isStreaming ? "chat-streaming" : ""}`}
          >
            {message.content.split("\n").map((line, i) => (
              <span key={i}>
                {line}
                {i < message.content.split("\n").length - 1 && <br />}
              </span>
            ))}
          </div>
        )}
        {message.toolCalls?.map((tc) => (
          <ToolCallBadge key={tc.id} tc={tc} />
        ))}
      </div>
    </div>
  );
}

const STATUS_INDICATOR: Record<string, string> = {
  idle: "",
  thinking: "\u25CF Thinking...",
  executing_tool: "\u2699 Executing tool...",
  error: "\u2717 Error",
  waiting_challenge: "\u26A0 CAPTCHA detected",
};

export function ChatPanel() {
  const {
    selectedId,
    messages,
    agentRunStatus,
    agentConnected,
    agentConfig,
    sendMessage,
    startAgent,
    stopAgent,
    shutdownAgent,
    clearHistory,
  } = useAgent();

  const [input, setInput] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  const [settingsConfig, setSettingsConfig] = useState(agentConfig);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const isBusy = agentRunStatus === "thinking" || agentRunStatus === "executing_tool";

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages.length, messages[messages.length - 1]?.content]);

  useEffect(() => {
    if (showSettings) {
      setSettingsConfig(agentConfig);
    }
  }, [showSettings, agentConfig]);

  const handleSend = useCallback(async () => {
    const text = input.trim();
    if (!text || isBusy) return;
    setInput("");
    await sendMessage(text);
  }, [input, isBusy, sendMessage]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  const handleStart = useCallback(async () => {
    if (!settingsConfig.apiKey) {
      return;
    }
    await startAgent(settingsConfig);
    setShowSettings(false);
  }, [settingsConfig, startAgent]);

  if (!selectedId) {
    return (
      <div className="chat-panel">
        <div className="chat-empty">Spawn an agent to start</div>
      </div>
    );
  }

  if (!agentConnected) {
    return (
      <div className="chat-panel">
        {showSettings ? (
          <div className="chat-settings">
            <div className="chat-settings-header">
              <span className="panel-title">Agent Configuration</span>
              <button
                className="btn-icon btn-icon-sm"
                onClick={() => setShowSettings(false)}
              >
                x
              </button>
            </div>
            <div className="chat-settings-body">
              <label className="chat-settings-label">
                API Key <span className="chat-settings-required">*</span>
              </label>
              <input
                className="chat-settings-input"
                type="password"
                value={settingsConfig.apiKey}
                onChange={(e) =>
                  setSettingsConfig((c) => ({ ...c, apiKey: e.target.value }))
                }
                placeholder="sk-..."
              />
              <label className="chat-settings-label">Model</label>
              <input
                className="chat-settings-input"
                type="text"
                value={settingsConfig.model}
                onChange={(e) =>
                  setSettingsConfig((c) => ({ ...c, model: e.target.value }))
                }
                placeholder="gpt-4"
              />
              <label className="chat-settings-label">Base URL</label>
              <input
                className="chat-settings-input"
                type="text"
                value={settingsConfig.baseURL}
                onChange={(e) =>
                  setSettingsConfig((c) => ({ ...c, baseURL: e.target.value }))
                }
                placeholder="https://api.openai.com/v1"
              />
              <div className="chat-settings-row">
                <div className="chat-settings-field">
                  <label className="chat-settings-label">Temperature</label>
                  <input
                    className="chat-settings-input"
                    type="number"
                    step="0.1"
                    min="0"
                    max="2"
                    value={settingsConfig.temperature}
                    onChange={(e) =>
                      setSettingsConfig((c) => ({
                        ...c,
                        temperature: parseFloat(e.target.value) || 0.7,
                      }))
                    }
                  />
                </div>
                <div className="chat-settings-field">
                  <label className="chat-settings-label">Max Rounds</label>
                  <input
                    className="chat-settings-input"
                    type="number"
                    min="1"
                    max="100"
                    value={settingsConfig.maxRounds}
                    onChange={(e) =>
                      setSettingsConfig((c) => ({
                        ...c,
                        maxRounds: parseInt(e.target.value) || 50,
                      }))
                    }
                  />
                </div>
              </div>
            </div>
            <div className="chat-settings-footer">
              <button
                className="btn btn-primary"
                disabled={!settingsConfig.apiKey}
                onClick={handleStart}
              >
                Connect Agent
              </button>
            </div>
          </div>
        ) : (
          <div className="chat-empty">
            <div className="chat-connect-prompt">
              <span className="chat-connect-icon">{"\u{1F916}"}</span>
              <span>Connect an AI agent to chat</span>
              <button
                className="btn btn-primary"
                style={{ marginTop: 12 }}
                onClick={() => setShowSettings(true)}
              >
                Configure Agent
              </button>
            </div>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="chat-panel">
      <div className="chat-toolbar">
        <span className="panel-title">Chat</span>
        {isBusy && (
          <span className="chat-status-badge">{STATUS_INDICATOR[agentRunStatus]}</span>
        )}
        {messages.length > 0 && (
          <span className="log-count">{messages.length}</span>
        )}
        <button
          className="btn-icon btn-icon-sm"
          title="Clear history"
          onClick={clearHistory}
          style={{ marginLeft: "auto" }}
        >
          {"\u{1F5D1}"}
        </button>
        <button
          className="btn-icon btn-icon-sm"
          title={isBusy ? "Stop" : "Disconnect"}
          onClick={isBusy ? stopAgent : shutdownAgent}
        >
          {isBusy ? "\u23F8" : "x"}
        </button>
      </div>
      <div className="chat-messages" ref={scrollRef}>
        {messages.length === 0 && (
          <div className="chat-welcome">
            <div>Ask the agent to browse the web for you.</div>
            <div className="chat-welcome-hints">
              <span className="chat-hint">"Go to google.com and search for rust programming"</span>
              <span className="chat-hint">"Find the pricing on example.com"</span>
              <span className="chat-hint">"Fill out the form on that page"</span>
            </div>
          </div>
        )}
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}
        {isBusy && (
          <div className="chat-msg chat-msg-assistant">
            <span className="chat-msg-role chat-role-assistant">agent</span>
            <div className="chat-msg-body">
              <span className="chat-cursor" />
            </div>
          </div>
        )}
      </div>
      <div className="chat-input-row">
        <textarea
          ref={inputRef}
          className="chat-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={
            isBusy
              ? "Agent is working..."
              : "Ask the agent to do something..."
          }
          disabled={isBusy}
          rows={1}
        />
        <button
          className="btn btn-primary btn-send"
          disabled={!input.trim() || isBusy}
          onClick={handleSend}
        >
          {"\u27A4"}
        </button>
      </div>
    </div>
  );
}
