import { describe, it, beforeEach } from 'node:test';
import assert from 'node:assert';
import { ToolExecutor } from '../../tools/executor.js';
import { createMockBrowserManager } from '../test-utils.js';
import type { BrowserManager } from '../../core/BrowserManager.js';

describe('ToolExecutor', () => {
  let executor: ToolExecutor;
  let manager: BrowserManager;

  beforeEach(() => {
    manager = createMockBrowserManager();
    executor = new ToolExecutor(manager);
  });

  describe('browser_new', () => {
    it('should create instance without options', async () => {
      const result = await executor.executeTool('browser_new', {});

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('created'));
    });

    it('should create instance with custom id', async () => {
      const result = await executor.executeTool('browser_new', {
        instance_id: 'my-browser',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('my-browser'));
    });

    it('should create instance with proxy', async () => {
      const result = await executor.executeTool('browser_new', {
        proxy: 'http://proxy.example.com:8080',
      });

      assert.strictEqual(result.success, true);
    });
  });

  describe('browser_navigate', () => {
    beforeEach(async () => {
      // Create instance first
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should navigate to URL', async () => {
      const result = await executor.executeTool('browser_navigate', {
        instance_id: 'test-browser',
        url: 'https://example.com',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Navigation Result'));
      assert.ok(result.content.includes('https://example.com'));
    });

    it('should fail for missing instance_id', async () => {
      const result = await executor.executeTool('browser_navigate', {
        url: 'https://example.com',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('instance_id'));
    });

    it('should fail for missing url', async () => {
      const result = await executor.executeTool('browser_navigate', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('url'));
    });

    it('should fail for non-existent instance', async () => {
      const result = await executor.executeTool('browser_navigate', {
        instance_id: 'non-existent',
        url: 'https://example.com',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('not found'));
    });

    it('should support wait_ms option', async () => {
      const result = await executor.executeTool('browser_navigate', {
        instance_id: 'test-browser',
        url: 'https://example.com',
        wait_ms: 5000,
      });

      assert.strictEqual(result.success, true);
    });

    it('should support interactive_only option', async () => {
      const result = await executor.executeTool('browser_navigate', {
        instance_id: 'test-browser',
        url: 'https://example.com',
        interactive_only: true,
      });

      assert.strictEqual(result.success, true);
    });
  });

  describe('browser_click', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should click element', async () => {
      const result = await executor.executeTool('browser_click', {
        instance_id: 'test-browser',
        element_id: '#1',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Click Result'));
    });

    it('should fail for missing instance_id', async () => {
      const result = await executor.executeTool('browser_click', {
        element_id: '#1',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('instance_id'));
    });

    it('should fail for missing element_id', async () => {
      const result = await executor.executeTool('browser_click', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('element_id'));
    });
  });

  describe('browser_fill', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should fill input', async () => {
      const result = await executor.executeTool('browser_fill', {
        instance_id: 'test-browser',
        element_id: '#2',
        value: 'search query',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Filled'));
      assert.ok(result.content.includes('search query'));
    });

    it('should fail for missing value', async () => {
      const result = await executor.executeTool('browser_fill', {
        instance_id: 'test-browser',
        element_id: '#2',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('value'));
    });
  });

  describe('browser_submit', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should submit form', async () => {
      const result = await executor.executeTool('browser_submit', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Submit Result'));
    });

    it('should submit specific form', async () => {
      const result = await executor.executeTool('browser_submit', {
        instance_id: 'test-browser',
        form_element_id: '#form1',
      });

      assert.strictEqual(result.success, true);
    });
  });

  describe('browser_scroll', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should scroll in all directions', async () => {
      const directions = ['up', 'down', 'top', 'bottom'] as const;

      for (const direction of directions) {
        const result = await executor.executeTool('browser_scroll', {
          instance_id: 'test-browser',
          direction,
        });

        assert.strictEqual(result.success, true, `Failed to scroll ${direction}`);
        assert.ok(result.content.includes(direction));
      }
    });

    it('should fail for invalid direction', async () => {
      // Type system should prevent this, but test anyway
      const result = await executor.executeTool('browser_scroll', {
        instance_id: 'test-browser',
        direction: 'sideways' as 'up',
      });

      // This will likely fail at runtime due to invalid direction
      assert.ok(!result.success || result.content.includes('sideways'));
    });
  });

  describe('browser_list', () => {
    it('should list empty when no instances', async () => {
      const result = await executor.executeTool('browser_list', {});

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('No active'));
    });

    it('should list active instances', async () => {
      // Create some instances
      await executor.executeTool('browser_new', { instance_id: 'browser-1' });
      await executor.executeTool('browser_new', { instance_id: 'browser-2' });

      const result = await executor.executeTool('browser_list', {});

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('browser-1'));
      assert.ok(result.content.includes('browser-2'));
    });
  });

  describe('browser_get_state', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should get current state', async () => {
      const result = await executor.executeTool('browser_get_state', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Current State'));
    });

    it('should fail for non-existent instance', async () => {
      const result = await executor.executeTool('browser_get_state', {
        instance_id: 'non-existent',
      });

      assert.strictEqual(result.success, false);
    });
  });

  describe('browser_close', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should close instance', async () => {
      const result = await executor.executeTool('browser_close', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('closed'));
    });

    it('should fail for non-existent instance', async () => {
      const result = await executor.executeTool('browser_close', {
        instance_id: 'non-existent',
      });

      assert.strictEqual(result.success, false);
    });
  });

  describe('unknown tool', () => {
    it('should return error for unknown tool', async () => {
      const result = await executor.executeTool(
        'browser_unknown' as 'browser_new',
        {}
      );

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('Unknown tool'));
    });
  });

  describe('browser_get_action_plan', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should get action plan', async () => {
      const result = await executor.executeTool('browser_get_action_plan', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Action Plan'));
      assert.ok(result.content.includes('Form Page'));
      assert.ok(result.content.includes('Suggested Actions'));
    });

    it('should show confidence scores', async () => {
      const result = await executor.executeTool('browser_get_action_plan', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('95%'));
    });

    it('should fail for missing instance_id', async () => {
      const result = await executor.executeTool('browser_get_action_plan', {});

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('instance_id'));
    });

    it('should fail for non-existent instance', async () => {
      const result = await executor.executeTool('browser_get_action_plan', {
        instance_id: 'non-existent',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('not found'));
    });
  });

  describe('browser_auto_fill', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should auto-fill fields', async () => {
      const result = await executor.executeTool('browser_auto_fill', {
        instance_id: 'test-browser',
        fields: [
          { key: 'email', value: 'test@example.com' },
          { key: 'password', value: 'secret' },
        ],
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Auto-Fill Result'));
      assert.ok(result.content.includes('Filled Fields'));
      assert.ok(result.content.includes('email'));
      assert.ok(result.content.includes('test@example.com'));
    });

    it('should show unmatched fields', async () => {
      const result = await executor.executeTool('browser_auto_fill', {
        instance_id: 'test-browser',
        fields: [{ key: 'email', value: 'test@example.com' }],
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Unmatched Fields'));
    });

    it('should fail for missing instance_id', async () => {
      const result = await executor.executeTool('browser_auto_fill', {
        fields: [{ key: 'email', value: 'test@example.com' }],
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('instance_id'));
    });

    it('should fail for missing fields', async () => {
      const result = await executor.executeTool('browser_auto_fill', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('fields'));
    });

    it('should fail for empty fields array', async () => {
      const result = await executor.executeTool('browser_auto_fill', {
        instance_id: 'test-browser',
        fields: [],
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('fields'));
    });

    it('should fail for non-existent instance', async () => {
      const result = await executor.executeTool('browser_auto_fill', {
        instance_id: 'non-existent',
        fields: [{ key: 'email', value: 'test@example.com' }],
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('not found'));
    });
  });

  describe('browser_wait', () => {
    beforeEach(async () => {
      await executor.executeTool('browser_new', { instance_id: 'test-browser' });
    });

    it('should wait with contentLoaded condition', async () => {
      const result = await executor.executeTool('browser_wait', {
        instance_id: 'test-browser',
        condition: 'contentLoaded',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('Wait Result'));
      assert.ok(result.content.includes('Satisfied'));
      assert.ok(result.content.includes('contentLoaded'));
    });

    it('should wait with contentStable condition', async () => {
      const result = await executor.executeTool('browser_wait', {
        instance_id: 'test-browser',
        condition: 'contentStable',
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('contentStable'));
    });

    it('should wait with minInteractive condition', async () => {
      const result = await executor.executeTool('browser_wait', {
        instance_id: 'test-browser',
        condition: 'minInteractive',
        min_count: 3,
      });

      assert.strictEqual(result.success, true);
      assert.ok(result.content.includes('minInteractive'));
    });

    it('should fail for missing instance_id', async () => {
      const result = await executor.executeTool('browser_wait', {
        condition: 'contentLoaded',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('instance_id'));
    });

    it('should fail for missing condition', async () => {
      const result = await executor.executeTool('browser_wait', {
        instance_id: 'test-browser',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('condition'));
    });

    it('should fail for selector condition without selector param', async () => {
      const result = await executor.executeTool('browser_wait', {
        instance_id: 'test-browser',
        condition: 'selector',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('selector'));
    });

    it('should fail for non-existent instance', async () => {
      const result = await executor.executeTool('browser_wait', {
        instance_id: 'non-existent',
        condition: 'contentLoaded',
      });

      assert.strictEqual(result.success, false);
      assert.ok(result.error?.includes('not found'));
    });
  });
});
