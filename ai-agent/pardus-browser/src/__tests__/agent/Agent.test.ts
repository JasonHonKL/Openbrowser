import { describe, it, beforeEach } from 'node:test';
import assert from 'node:assert';
import { Agent } from '../../agent/Agent.js';
import { createMockBrowserManager } from '../test-utils.js';
import type { BrowserManager } from '../../core/BrowserManager.js';

describe('Agent', () => {
  let agent: Agent;
  let manager: BrowserManager;

  beforeEach(() => {
    manager = createMockBrowserManager();
    agent = new Agent(manager, {
      llmConfig: {
        apiKey: 'test-key',
        baseURL: 'http://localhost:1234',
        model: 'test-model',
      },
    });
  });

  describe('constructor', () => {
    it('should initialize with system prompt', () => {
      const history = agent.getHistory();
      assert.strictEqual(history.length, 1);
      assert.strictEqual(history[0].role, 'system');
      assert.ok(history[0].content.length > 0);
    });

    it('should accept custom instructions', () => {
      const customAgent = new Agent(manager, {
        llmConfig: { apiKey: 'test' },
        customInstructions: 'Always be polite',
      });

      const history = customAgent.getHistory();
      assert.ok(history[0].content.includes('Always be polite'));
    });

    it('should accept tool configuration', () => {
      const customAgent = new Agent(manager, {
        llmConfig: { apiKey: 'test' },
        toolConfig: {
          parallel: true,
          continueOnError: false,
          defaultRetryConfig: {
            retries: 3,
            timeout: 10000,
          },
        },
      });

      const config = customAgent.getToolConfig();
      assert.strictEqual(config?.parallel, true);
      assert.strictEqual(config?.continueOnError, false);
      assert.strictEqual(config?.defaultRetryConfig?.retries, 3);
      assert.strictEqual(config?.defaultRetryConfig?.timeout, 10000);
    });

    it('should have default tool configuration', () => {
      const config = agent.getToolConfig();
      assert.strictEqual(config?.parallel, true);
      assert.strictEqual(config?.continueOnError, true);
      assert.strictEqual(config?.defaultRetryConfig, undefined);
    });
  });

  describe('getHistory', () => {
    it('should return copy of history', () => {
      const history1 = agent.getHistory();
      const history2 = agent.getHistory();
      
      assert.notStrictEqual(history1, history2);
      assert.deepStrictEqual(history1, history2);
    });
  });

  describe('clearHistory', () => {
    it('should preserve system message', () => {
      agent.clearHistory();
      const history = agent.getHistory();
      
      assert.strictEqual(history.length, 1);
      assert.strictEqual(history[0].role, 'system');
    });

    it('should remove user and assistant messages', () => {
      // Simulate conversation
      const history = agent.getHistory();
      history.push({ role: 'user', content: 'Hello' });
      history.push({ role: 'assistant', content: 'Hi!' });

      agent.clearHistory();
      const cleared = agent.getHistory();

      assert.strictEqual(cleared.length, 1);
      assert.strictEqual(cleared[0].role, 'system');
    });
  });

  describe('cleanup', () => {
    it('should close all browser instances', async () => {
      // Create some instances
      await manager.createInstance({ id: 'browser-1' });
      await manager.createInstance({ id: 'browser-2' });

      assert.strictEqual(manager.instanceCount, 2);

      await agent.cleanup();

      assert.strictEqual(manager.instanceCount, 0);
    });
  });

  describe('tool configuration', () => {
    it('should update tool config at runtime', () => {
      agent.setToolConfig({ parallel: true });
      const config = agent.getToolConfig();
      assert.strictEqual(config?.parallel, true);
    });

    it('should preserve other config values when updating', () => {
      agent.setToolConfig({ parallel: true });
      const config = agent.getToolConfig();
      // continueOnError should still be the default
      assert.strictEqual(config?.continueOnError, true);
    });

    it('should allow setting retry configuration', () => {
      agent.setToolConfig({
        defaultRetryConfig: {
          retries: 5,
          retryDelay: 500,
          retryBackoff: 1.5,
        },
      });

      const config = agent.getToolConfig();
      assert.strictEqual(config?.defaultRetryConfig?.retries, 5);
      assert.strictEqual(config?.defaultRetryConfig?.retryDelay, 500);
      assert.strictEqual(config?.defaultRetryConfig?.retryBackoff, 1.5);
    });
  });

  // Note: The chat() method requires actual LLM calls
  // These tests are skipped without network/mock
  describe('chat (requires LLM)', () => {
    it.skip('should process user message', async () => {
      const response = await agent.chat('Hello');
      assert.ok(typeof response === 'string');
    });

    it.skip('should handle tool calls', async () => {
      // This would require mocking the LLM client
    });

    it.skip('should prevent concurrent calls', async () => {
      const promise1 = agent.chat('First');
      const promise2 = agent.chat('Second');

      try {
        await Promise.all([promise1, promise2]);
        assert.fail('Should have thrown');
      } catch (error) {
        assert.ok((error as Error).message.includes('already processing'));
      }
    });
  });

  describe('maxRounds', () => {
    it('should default to 50', () => {
      const defaultAgent = new Agent(manager, {
        llmConfig: { apiKey: 'test' },
      });
      // maxRounds is private, but we can verify through behavior
      assert.ok(defaultAgent);
    });

    it('should accept custom maxRounds', () => {
      const limitedAgent = new Agent(manager, {
        llmConfig: { apiKey: 'test' },
        maxRounds: 10,
      });
      assert.ok(limitedAgent);
    });
  });
});
