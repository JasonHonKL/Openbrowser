import { describe, it } from 'node:test';
import assert from 'node:assert';
import {
  canExecuteInParallel,
  groupToolsForParallelExecution,
} from '../../tools/types.js';

describe('Tool Execution Types', () => {
  describe('canExecuteInParallel', () => {
    it('should allow parallel execution for tools on different instances', () => {
      const tool1 = {
        name: 'browser_navigate',
        args: { instance_id: 'browser-a', url: 'https://example.com' },
      };
      const tool2 = {
        name: 'browser_navigate',
        args: { instance_id: 'browser-b', url: 'https://example.com' },
      };

      assert.strictEqual(canExecuteInParallel(tool1, tool2), true);
    });

    it('should allow parallel for read-only tools on same instance', () => {
      const tool1 = {
        name: 'browser_get_state',
        args: { instance_id: 'browser-a' },
      };
      const tool2 = {
        name: 'browser_list',
        args: { instance_id: 'browser-a' },
      };

      assert.strictEqual(canExecuteInParallel(tool1, tool2), true);
    });

    it('should NOT allow parallel for write operations on same instance', () => {
      const tool1 = {
        name: 'browser_navigate',
        args: { instance_id: 'browser-a', url: 'https://example1.com' },
      };
      const tool2 = {
        name: 'browser_click',
        args: { instance_id: 'browser-a', element_id: '#1' },
      };

      assert.strictEqual(canExecuteInParallel(tool1, tool2), false);
    });

    it('should allow parallel if one tool has no instance', () => {
      const tool1 = {
        name: 'browser_list',
        args: {},
      };
      const tool2 = {
        name: 'browser_navigate',
        args: { instance_id: 'browser-a', url: 'https://example.com' },
      };

      assert.strictEqual(canExecuteInParallel(tool1, tool2), true);
    });

    it('should NOT allow parallel for different write operations on same instance', () => {
      const tool1 = {
        name: 'browser_fill',
        args: { instance_id: 'browser-a', element_id: '#1', value: 'test' },
      };
      const tool2 = {
        name: 'browser_click',
        args: { instance_id: 'browser-a', element_id: '#2' },
      };

      assert.strictEqual(canExecuteInParallel(tool1, tool2), false);
    });
  });

  describe('groupToolsForParallelExecution', () => {
    it('should group independent tools together', () => {
      const tools = [
        { name: 'browser_navigate', args: { instance_id: 'a', url: 'https://a.com' } },
        { name: 'browser_navigate', args: { instance_id: 'b', url: 'https://b.com' } },
        { name: 'browser_navigate', args: { instance_id: 'c', url: 'https://c.com' } },
      ];

      const groups = groupToolsForParallelExecution(tools);

      // All three can run in parallel since they're on different instances
      assert.strictEqual(groups.length, 1);
      assert.strictEqual(groups[0].tools.length, 3);
    });

    it('should separate conflicting tools into different groups', () => {
      const tools = [
        { name: 'browser_navigate', args: { instance_id: 'a', url: 'https://example.com' } },
        { name: 'browser_click', args: { instance_id: 'a', element_id: '#1' } },
        { name: 'browser_navigate', args: { instance_id: 'b', url: 'https://other.com' } },
      ];

      const groups = groupToolsForParallelExecution(tools);

      // First group: navigate(a) + navigate(b) (different instances, can parallel)
      // Second group: click(a) (conflicts with navigate(a) on same instance)
      assert.strictEqual(groups.length, 2);
    });

    it('should handle empty tool list', () => {
      const groups = groupToolsForParallelExecution([]);
      assert.strictEqual(groups.length, 0);
    });

    it('should handle single tool', () => {
      const tools = [
        { name: 'browser_navigate', args: { instance_id: 'a', url: 'https://example.com' } },
      ];

      const groups = groupToolsForParallelExecution(tools);
      assert.strictEqual(groups.length, 1);
      assert.strictEqual(groups[0].tools.length, 1);
    });

    it('should group read-only operations on same instance together', () => {
      const tools = [
        { name: 'browser_get_state', args: { instance_id: 'a' } },
        { name: 'browser_get_cookies', args: { instance_id: 'a' } },
        { name: 'browser_list', args: {} },
      ];

      const groups = groupToolsForParallelExecution(tools);

      // All read-only can run in parallel
      assert.strictEqual(groups.length, 1);
      assert.strictEqual(groups[0].tools.length, 3);
    });

    it('should treat browser_get_action_plan as read-only', () => {
      const tool1 = {
        name: 'browser_get_action_plan',
        args: { instance_id: 'browser-a' },
      };
      const tool2 = {
        name: 'browser_get_state',
        args: { instance_id: 'browser-a' },
      };

      assert.strictEqual(canExecuteInParallel(tool1, tool2), true);
    });

    it('should NOT allow parallel for browser_auto_fill on same instance', () => {
      const tool1 = {
        name: 'browser_auto_fill',
        args: { instance_id: 'browser-a', fields: [{ key: 'email', value: 'a@b.com' }] },
      };
      const tool2 = {
        name: 'browser_click',
        args: { instance_id: 'browser-a', element_id: '#1' },
      };

      assert.strictEqual(canExecuteInParallel(tool1, tool2), false);
    });
  });
});
