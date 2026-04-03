import { describe, it } from 'node:test';
import assert from 'node:assert';
import { SYSTEM_PROMPT, getSystemPrompt } from '../../llm/prompts.js';

describe('Prompts', () => {
  describe('SYSTEM_PROMPT', () => {
    it('should be defined', () => {
      assert.ok(SYSTEM_PROMPT);
      assert.ok(SYSTEM_PROMPT.length > 0);
    });

    it('should explain browser instances', () => {
      assert.ok(SYSTEM_PROMPT.includes('browser instance'));
      assert.ok(SYSTEM_PROMPT.includes('isolated'));
    });

    it('should explain semantic tree', () => {
      assert.ok(SYSTEM_PROMPT.includes('semantic tree'));
      assert.ok(SYSTEM_PROMPT.includes('Element IDs'));
    });

    it('should list available tools', () => {
      assert.ok(SYSTEM_PROMPT.includes('browser_new'));
      assert.ok(SYSTEM_PROMPT.includes('browser_navigate'));
      assert.ok(SYSTEM_PROMPT.includes('browser_click'));
      assert.ok(SYSTEM_PROMPT.includes('browser_fill'));
      assert.ok(SYSTEM_PROMPT.includes('browser_submit'));
      assert.ok(SYSTEM_PROMPT.includes('browser_scroll'));
      assert.ok(SYSTEM_PROMPT.includes('browser_close'));
      assert.ok(SYSTEM_PROMPT.includes('browser_list'));
      assert.ok(SYSTEM_PROMPT.includes('browser_get_state'));
      assert.ok(SYSTEM_PROMPT.includes('browser_get_action_plan'));
      assert.ok(SYSTEM_PROMPT.includes('browser_auto_fill'));
      assert.ok(SYSTEM_PROMPT.includes('browser_wait'));
      assert.ok(SYSTEM_PROMPT.includes('browser_get_cookies'));
      assert.ok(SYSTEM_PROMPT.includes('browser_set_cookie'));
    });

    it('should mention correct tool count', () => {
      assert.ok(SYSTEM_PROMPT.includes('19 browser tools'));
    });

    it('should have workflow steps', () => {
      assert.ok(SYSTEM_PROMPT.includes('browser_new()'));
      assert.ok(SYSTEM_PROMPT.includes('browser_navigate()'));
      assert.ok(SYSTEM_PROMPT.includes('browser_click()'));
    });

    it('should explain element IDs', () => {
      assert.ok(SYSTEM_PROMPT.includes('[#1]'));
      assert.ok(SYSTEM_PROMPT.includes('Element IDs'));
    });

    it('should include best practices', () => {
      assert.ok(SYSTEM_PROMPT.includes('Best Practices'));
    });
  });

  describe('getSystemPrompt', () => {
    it('should return default prompt without custom instructions', () => {
      const prompt = getSystemPrompt();
      assert.strictEqual(prompt, SYSTEM_PROMPT);
    });

    it('should append custom instructions', () => {
      const custom = 'Always verify prices are in USD';
      const prompt = getSystemPrompt(custom);

      assert.ok(prompt.includes(SYSTEM_PROMPT));
      assert.ok(prompt.includes('Additional Instructions'));
      assert.ok(prompt.includes(custom));
    });

    it('should handle empty custom instructions', () => {
      const prompt = getSystemPrompt('');
      assert.ok(prompt.includes(SYSTEM_PROMPT));
    });
  });
});
