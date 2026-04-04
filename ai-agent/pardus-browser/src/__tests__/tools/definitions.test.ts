import { describe, it } from 'node:test';
import assert from 'node:assert';
import { browserTools, BrowserToolName } from '../../tools/definitions.js';

describe('Tool Definitions', () => {
  describe('browserTools', () => {
    it('should have 40 tools', () => {
      assert.strictEqual(browserTools.length, 40);
    });

    it('should include all expected tools', () => {
      const toolNames = browserTools.map(t => t.function.name);
      const expectedTools = [
        'browser_new',
        'browser_navigate',
        'browser_click',
        'browser_fill',
        'browser_submit',
        'browser_scroll',
        'browser_get_cookies',
        'browser_set_cookie',
        'browser_delete_cookie',
        'browser_get_storage',
        'browser_set_storage',
        'browser_delete_storage',
        'browser_clear_storage',
        'browser_get_action_plan',
        'browser_auto_fill',
        'browser_wait',
        'browser_get_state',
        'browser_list',
        'browser_close',
      ];

      for (const expected of expectedTools) {
        assert.ok(toolNames.includes(expected), `Missing tool: ${expected}`);
      }
    });
  });

  describe('browser_new', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_new')!;

    it('should have correct structure', () => {
      assert.strictEqual(tool.type, 'function');
      assert.strictEqual(tool.function.name, 'browser_new');
      assert.ok(tool.function.description);
    });

    it('should have optional instance_id parameter', () => {
      const params = tool.function.parameters;
      assert.ok('instance_id' in params.properties);
      const required = tool.function.parameters.required || [];
      assert.ok(!required.includes('instance_id'));
    });

    it('should have optional proxy parameter', () => {
      const params = tool.function.parameters;
      assert.ok('proxy' in params.properties);
    });

    it('should have optional timeout parameter', () => {
      const params = tool.function.parameters;
      assert.ok('timeout' in params.properties);
    });
  });

  describe('browser_navigate', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_navigate')!;

    it('should require instance_id and url', () => {
      const required = tool.function.parameters.required || [];
      assert.ok(required.includes('instance_id'));
      assert.ok(required.includes('url'));
    });

    it('should have optional wait_ms parameter', () => {
      const params = tool.function.parameters;
      assert.ok('wait_ms' in params.properties);
    });

    it('should have optional interactive_only parameter', () => {
      const params = tool.function.parameters;
      assert.ok('interactive_only' in params.properties);
    });

    it('should have optional headers parameter', () => {
      const params = tool.function.parameters;
      assert.ok('headers' in params.properties);
    });
  });

  describe('browser_click', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_click')!;

    it('should require instance_id and element_id', () => {
      const required = tool.function.parameters.required || [];
      assert.ok(required.includes('instance_id'));
      assert.ok(required.includes('element_id'));
    });
  });

  describe('browser_fill', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_fill')!;

    it('should require instance_id, element_id, and value', () => {
      const required = tool.function.parameters.required || [];
      assert.ok(required.includes('instance_id'));
      assert.ok(required.includes('element_id'));
      assert.ok(required.includes('value'));
    });
  });

  describe('browser_submit', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_submit')!;

    it('should require only instance_id', () => {
      const required = tool.function.parameters.required || [];
      assert.ok(required.includes('instance_id'));
      assert.ok(!required.includes('form_element_id'));
    });

    it('should have optional form_element_id', () => {
      const params = tool.function.parameters;
      assert.ok('form_element_id' in params.properties);
    });
  });

  describe('browser_scroll', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_scroll')!;

    it('should have direction enum', () => {
      const params = tool.function.parameters;
      const directionProp = params.properties.direction as { enum: string[] };
      assert.deepStrictEqual(directionProp.enum, ['up', 'down', 'top', 'bottom']);
    });
  });

  describe('browser_list', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_list')!;

    it('should have no parameters', () => {
      assert.deepStrictEqual(tool.function.parameters.properties, {});
      const required = tool.function.parameters.required || [];
      assert.deepStrictEqual(required, []);
    });
  });

  describe('browser_get_state', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_get_state')!;

    it('should require instance_id', () => {
      const required = tool.function.parameters.required || [];
      assert.ok(required.includes('instance_id'));
    });
  });

  describe('browser_get_action_plan', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_get_action_plan')!;

    it('should exist', () => {
      assert.ok(tool);
    });

    it('should require instance_id', () => {
      const required = tool.function.parameters.required || [];
      assert.ok(required.includes('instance_id'));
    });

    it('should describe action planning', () => {
      assert.ok(tool.function.description.includes('action plan'));
      assert.ok(tool.function.description.includes('confidence'));
    });
  });

  describe('browser_auto_fill', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_auto_fill')!;

    it('should exist', () => {
      assert.ok(tool);
    });

    it('should require instance_id and fields', () => {
      const required = tool.function.parameters.required || [];
      assert.ok(required.includes('instance_id'));
      assert.ok(required.includes('fields'));
    });

    it('should have fields as array', () => {
      const fields = tool.function.parameters.properties.fields as { type: string; items: { properties: Record<string, unknown> } };
      assert.strictEqual(fields.type, 'array');
      assert.ok(fields.items.properties.key);
      assert.ok(fields.items.properties.value);
    });
  });

  describe('browser_wait', () => {
    const tool = browserTools.find(t => t.function.name === 'browser_wait')!;

    it('should exist', () => {
      assert.ok(tool);
    });

    it('should require instance_id and condition', () => {
      const required = tool.function.parameters.required || [];
      assert.ok(required.includes('instance_id'));
      assert.ok(required.includes('condition'));
    });

    it('should have condition enum with all wait types', () => {
      const conditionProp = tool.function.parameters.properties.condition as { enum: string[] };
      assert.ok(conditionProp.enum.includes('contentLoaded'));
      assert.ok(conditionProp.enum.includes('contentStable'));
      assert.ok(conditionProp.enum.includes('networkIdle'));
      assert.ok(conditionProp.enum.includes('minInteractive'));
      assert.ok(conditionProp.enum.includes('selector'));
    });

    it('should have optional selector parameter', () => {
      assert.ok('selector' in tool.function.parameters.properties);
    });

    it('should have optional timeout_ms parameter', () => {
      assert.ok('timeout_ms' in tool.function.parameters.properties);
    });
  });

  describe('BrowserToolName', () => {
    it('should be a union of all tool names', () => {
      const toolNames: BrowserToolName[] = [
        'browser_new',
        'browser_navigate',
        'browser_click',
        'browser_fill',
        'browser_submit',
        'browser_scroll',
        'browser_get_cookies',
        'browser_set_cookie',
        'browser_delete_cookie',
        'browser_get_storage',
        'browser_set_storage',
        'browser_delete_storage',
        'browser_clear_storage',
        'browser_get_action_plan',
        'browser_auto_fill',
        'browser_wait',
        'browser_get_state',
        'browser_list',
        'browser_close',
        'browser_extract_text',
        'browser_extract_links',
        'browser_find',
        'browser_extract_table',
        'browser_extract_metadata',
        'browser_screenshot',
        'browser_select',
        'browser_press_key',
        'browser_hover',
        'browser_tab_new',
        'browser_tab_switch',
        'browser_tab_close',
        'browser_download',
        'browser_upload',
        'browser_pdf_extract',
        'browser_feed_parse',
        'browser_network_block',
        'browser_network_log',
        'browser_iframe_enter',
        'browser_iframe_exit',
        'browser_diff',
      ];

      assert.strictEqual(toolNames.length, 40);
    });
  });
});
