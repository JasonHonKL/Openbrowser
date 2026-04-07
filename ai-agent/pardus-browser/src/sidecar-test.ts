// Manual integration test for sidecar JSON-RPC protocol.
// Run: node dist/sidecar-test.js < input.jsonl
//
// Input format: one JSON-RPC request per line (no trailing newline on last line)

import { execSync } from "child_process";

const sidecarPath = process.argv[2] || "node dist/sidecar.js";

const tests = [
  {
    name: "agent.init with valid config",
    input: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "agent.init",
      params: {
        apiKey: "test-key",
        model: "gpt-4",
        baseURL: "https://api.openai.com/v1",
        temperature: 0.5,
        maxTokens: 1000,
        maxRounds: 10,
      },
    }),
    expect: (lines) => {
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 1) return `expected id=1, got ${resp.id}`;
      if (resp.result?.ok !== true) return `expected ok=true, got ${JSON.stringify(resp.result)}`;
      return null;
    },
  },
  {
    name: "agent.init without apiKey should fail",
    input: JSON.stringify({
      jsonrpc: "2.0",
      id: 2,
      method: "agent.init",
      params: {},
    }),
    expect: (lines) => {
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 2) return `expected id=2, got ${resp.id}`;
      if (!resp.error) return `expected error, got ${JSON.stringify(resp)}`;
      return null;
    },
  },
  {
    name: "agent.init with extra init message (id=1 reserved)",
    input: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "agent.init",
      params: {
        apiKey: "test-key",
        model: "gpt-4",
      },
    }),
    expect: (lines) => {
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 1) return `expected id=1, got ${resp.id}`;
      if (resp.result?.ok !== true) return `expected ok=true, got ${JSON.stringify(resp.result)}`;
      return null;
    },
  },
  {
    name: "agent.stop after init",
    input: JSON.stringify({
      jsonrpc: "2.0",
      id: 3,
      method: "agent.stop",
      params: {},
    }),
    expect: (lines) => {
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 3) return `expected id=3, got ${resp.id}`;
      if (resp.result?.ok !== true) return `expected ok=true, got ${JSON.stringify(resp.result)}`;
      return null;
    },
  },
  {
    name: "unknown method returns error",
    input: JSON.stringify({
      jsonrpc: "2.0",
      id: 4,
      method: "agent.unknown",
      params: {},
    }),
    expect: (lines) => {
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 4) return `expected id=4, got ${resp.id}`;
      if (!resp.error) return `expected error, got ${JSON.stringify(resp)}`;
      if (resp.error.code !== -32601) return `expected code -32601, got ${resp.error.code}`;
      return null;
    },
  },
  {
    name: "chat without init should fail",
    input: JSON.stringify({
      jsonrpc: "2.0",
      id: 5,
      method: "agent.chat",
      params: { message: "hello" },
    }),
    expect: (lines) => {
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 5) return `expected id=5, got ${resp.id}`;
      if (!resp.error) return `expected error, got ${JSON.stringify(resp)}`;
      return null;
    },
  },
  {
    name: "clearHistory without init should fail",
    input: JSON.stringify({
      jsonrpc: "2.0",
      id: 6,
      method: "agent.clearHistory",
      params: {},
    }),
    expect: (lines) => {
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 6) return `expected id=6, got ${resp.id}`;
      if (!resp.error) return `expected error, got ${JSON.stringify(resp)}`;
      return null;
    },
  },
  {
    name: "getHistory without init should fail",
    input: JSON.stringify({
      jsonrpc: "2.0",
      id: 7,
      method: "agent.getHistory",
      params: {},
    }),
    expect: (lines) => {
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 7) return `expected id=7, got ${resp.id}`;
      if (!resp.error) return `expected error, got ${JSON.stringify(resp)}`;
      return null;
    },
  },
  {
    name: "malformed JSON is ignored",
    input: "not json at all\n" + JSON.stringify({
      jsonrpc: "2.0",
      id: 8,
      method: "agent.init",
      params: { apiKey: "test-key", model: "gpt-4" },
    }),
    expect: (lines) => {
      if (lines.length < 2) return `expected 2 lines (first ignored), got ${lines.length}`;
      const resp = JSON.parse(lines[1]);
      if (resp.id !== 8) return `expected id=8, got ${resp.id}`;
      if (resp.result?.ok !== true) return `expected ok=true, got ${JSON.stringify(resp.result)}`;
      return null;
    },
  },
  {
    name: "empty lines are ignored",
    input: "\n\n" + JSON.stringify({
      jsonrpc: "2.0",
      id: 9,
      method: "agent.init",
      params: { apiKey: "test-key", model: "gpt-4" },
    }) + "\n\n",
    expect: (lines) => {
      if (lines.length < 1) return `expected at least 1 line, got ${lines.length}`;
      const resp = JSON.parse(lines[0]);
      if (resp.id !== 9) return `expected id=9, got ${resp.id}`;
      if (resp.result?.ok !== true) return `expected ok=true, got ${JSON.stringify(resp.result)}`;
      return null;
    },
  },
  {
    name: "notification without id is handled",
    input: JSON.stringify({
      jsonrpc: "2.0",
      method: "agent.init",
      params: { apiKey: "test-key", model: "gpt-4" },
    }),
    expect: (lines) => {
      // Notification (no id) should not produce a response, so lines is empty
      if (lines.length !== 0) return `expected 0 lines (notification), got ${lines.length}: ${lines.join(", ")}`;
      return null;
    },
  },
];

let passed = 0;
let failed = 0;
const errors: string[] = [];

for (const test of tests) {
  console.log(`\n  ${test.name}...`);

  try {
    const child = execSync(`node ${sidecarPath}`, {
      input: test.input + "\n",
      timeout: 5000,
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });

    const stdout = child.stdout?.toString().trim();
    const lines = stdout.split("\n").filter((l) => l.length > 0);

    const err = test.expect(lines);
    if (err) {
      console.log(`    FAIL: ${err}`);
      if (stdout) console.log(`    stdout: ${stdout.slice(0, 200)}`);
      failed++;
      errors.push(test.name);
    } else {
      console.log(`    OK`);
      passed++;
    }
  } catch (e) {
    // test 9 and 11 (init-dependent) might fail because init was already called
    // but since we spawn a new process each time, they should all work
    console.log(`    FAIL: ${e instanceof Error ? e.message : String(e)}`);
    failed++;
    errors.push(test.name);
  }
}

console.log(`\n${"=".repeat(50)}`);
console.log(`  Results: ${passed} passed, ${failed} failed, ${tests.length} total`);
if (errors.length > 0) {
  console.log(`  Failed: ${errors.join(", ")}`);
}

process.exit(failed > 0 ? 1 : 0);
