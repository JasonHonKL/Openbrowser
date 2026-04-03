/**
 * OpenAI function calling tool definitions
 * These are passed to the LLM to describe available browser tools
 */

export interface ToolDefinition {
  type: 'function';
  function: {
    name: string;
    description: string;
    parameters: {
      type: 'object';
      properties: Record<string, unknown>;
      required?: string[];
    };
  };
}

export const browserTools: ToolDefinition[] = [
  {
    type: 'function',
    function: {
      name: 'browser_new',
      description: 'Create a new browser instance. Each instance maintains its own session (cookies, localStorage, history). Returns an instance_id used for subsequent calls.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'Optional custom ID for the browser instance. If not provided, a random ID will be generated.',
          },
          proxy: {
            type: 'string',
            description: 'Optional proxy URL (e.g., "http://proxy.example.com:8080" or "socks5://user:pass@host:1080")',
          },
          timeout: {
            type: 'number',
            description: 'Optional timeout in milliseconds for browser operations (default: 30000)',
          },
        },
        required: [],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_navigate',
      description: 'Navigate to a URL and return the semantic tree. The semantic tree shows interactive elements with IDs like [#1], [#2] that can be clicked or filled.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          url: {
            type: 'string',
            description: 'Full URL to navigate to (e.g., "https://example.com")',
          },
          wait_ms: {
            type: 'number',
            description: 'Optional wait time in milliseconds for JavaScript execution (default: 3000)',
          },
          interactive_only: {
            type: 'boolean',
            description: 'If true, only return interactive elements (links, buttons, inputs) - useful for crowded pages',
          },
          headers: {
            type: 'object',
            description: 'Optional custom HTTP headers to send with the request (e.g., {"Authorization": "Bearer token"})',
          },
        },
        required: ['instance_id', 'url'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_click',
      description: 'Click an element by its ID from the semantic tree. Returns the updated page state after the click.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          element_id: {
            type: 'string',
            description: 'Element ID from the semantic tree (e.g., "#1", "#2")',
          },
        },
        required: ['instance_id', 'element_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_fill',
      description: 'Fill a text input or textarea with a value. The element should be a textbox from the semantic tree.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          element_id: {
            type: 'string',
            description: 'Element ID of the input field (e.g., "#3")',
          },
          value: {
            type: 'string',
            description: 'Value to fill into the input',
          },
        },
        required: ['instance_id', 'element_id', 'value'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_submit',
      description: 'Submit a form. If form_element_id is not provided, submits the first form on the page.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          form_element_id: {
            type: 'string',
            description: 'Optional: Element ID of the form to submit. If omitted, submits the first form.',
          },
        },
        required: ['instance_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_scroll',
      description: 'Scroll the page in a direction.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          direction: {
            type: 'string',
            enum: ['up', 'down', 'top', 'bottom'],
            description: 'Direction to scroll: up (one screen), down (one screen), top (to page top), bottom (to page end)',
          },
        },
        required: ['instance_id', 'direction'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_get_cookies',
      description: 'Get all cookies for the current page or a specific URL.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          url: {
            type: 'string',
            description: 'Optional: URL to get cookies for. If omitted, returns cookies for current page.',
          },
        },
        required: ['instance_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_set_cookie',
      description: 'Set a cookie for a specific URL.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          name: {
            type: 'string',
            description: 'Cookie name',
          },
          value: {
            type: 'string',
            description: 'Cookie value',
          },
          url: {
            type: 'string',
            description: 'URL to set cookie for (defaults to current page)',
          },
          domain: {
            type: 'string',
            description: 'Optional: Cookie domain',
          },
          path: {
            type: 'string',
            description: 'Optional: Cookie path (default: "/")',
          },
          expires: {
            type: 'number',
            description: 'Optional: Unix timestamp when cookie expires',
          },
          httpOnly: {
            type: 'boolean',
            description: 'Optional: HttpOnly flag',
          },
          secure: {
            type: 'boolean',
            description: 'Optional: Secure flag',
          },
          sameSite: {
            type: 'string',
            enum: ['Strict', 'Lax', 'None'],
            description: 'Optional: SameSite attribute',
          },
        },
        required: ['instance_id', 'name', 'value'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_delete_cookie',
      description: 'Delete a cookie by name for a specific URL.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          name: {
            type: 'string',
            description: 'Cookie name to delete',
          },
          url: {
            type: 'string',
            description: 'Optional: URL to delete cookie from (defaults to current page)',
          },
        },
        required: ['instance_id', 'name'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_get_storage',
      description: 'Get items from localStorage or sessionStorage.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          storage_type: {
            type: 'string',
            enum: ['localStorage', 'sessionStorage'],
            description: 'Type of storage to read from',
          },
          key: {
            type: 'string',
            description: 'Optional: Specific key to read. If omitted, returns all items.',
          },
        },
        required: ['instance_id', 'storage_type'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_set_storage',
      description: 'Set an item in localStorage or sessionStorage.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          storage_type: {
            type: 'string',
            enum: ['localStorage', 'sessionStorage'],
            description: 'Type of storage to write to',
          },
          key: {
            type: 'string',
            description: 'Key to set',
          },
          value: {
            type: 'string',
            description: 'Value to set (will be JSON stringified if object)',
          },
        },
        required: ['instance_id', 'storage_type', 'key', 'value'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_delete_storage',
      description: 'Remove an item from localStorage or sessionStorage.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          storage_type: {
            type: 'string',
            enum: ['localStorage', 'sessionStorage'],
            description: 'Type of storage to delete from',
          },
          key: {
            type: 'string',
            description: 'Key to remove',
          },
        },
        required: ['instance_id', 'storage_type', 'key'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_clear_storage',
      description: 'Clear all items from localStorage or sessionStorage.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          storage_type: {
            type: 'string',
            enum: ['localStorage', 'sessionStorage', 'both'],
            description: 'Type of storage to clear',
          },
        },
        required: ['instance_id', 'storage_type'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_get_state',
      description: 'Get the current page state (URL, title, semantic tree) without navigating.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
        },
        required: ['instance_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_get_action_plan',
      description: 'Get an AI-optimized action plan for the current page. Returns page type classification (login, search, form, listing, etc.), a prioritized list of suggested actions with confidence scores, and whether the page has forms or pagination. Use after navigating to decide what to do next.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
        },
        required: ['instance_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_auto_fill',
      description: 'Auto-fill form fields on the current page using smart matching (by field name, label, placeholder, or input type). Returns which fields were filled and which were unmatched. Use when a form has multiple fields to fill efficiently.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          fields: {
            type: 'array',
            items: {
              type: 'object',
              properties: {
                key: {
                  type: 'string',
                  description: 'Field name, label, or type to match (e.g., "email", "username", "password")',
                },
                value: {
                  type: 'string',
                  description: 'Value to fill into the matched field',
                },
              },
              required: ['key', 'value'],
            },
            description: 'Array of key-value pairs to fill into form fields',
          },
        },
        required: ['instance_id', 'fields'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_wait',
      description: 'Wait for a smart condition on the current page instead of using a fixed wait_ms. Prefer this over wait_ms for SPAs and dynamic pages. Conditions: contentLoaded (waits for no spinners/skeletons + substantial content), contentStable (waits for DOM to stop changing), networkIdle (longer stable wait for lazy-loaded content), minInteractive (waits for N interactive elements to appear), selector (waits for a CSS selector to appear).',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          condition: {
            type: 'string',
            enum: ['contentLoaded', 'contentStable', 'networkIdle', 'minInteractive', 'selector'],
            description: 'The wait condition to use',
          },
          selector: {
            type: 'string',
            description: 'Required when condition is "selector": the CSS selector to wait for',
          },
          min_count: {
            type: 'number',
            description: 'Required when condition is "minInteractive": minimum number of interactive elements to wait for (default: 1)',
          },
          timeout_ms: {
            type: 'number',
            description: 'Maximum wait time in milliseconds (default: 10000)',
          },
          interval_ms: {
            type: 'number',
            description: 'Polling interval in milliseconds (default: 500)',
          },
        },
        required: ['instance_id', 'condition'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_list',
      description: 'List all active browser instances with their current URLs and connection status.',
      parameters: {
        type: 'object',
        properties: {},
        required: [],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_close',
      description: 'Close a browser instance and clean up resources.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID to close',
          },
        },
        required: ['instance_id'],
      },
    },
  },
];

export type BrowserToolName = 
  | 'browser_new'
  | 'browser_navigate'
  | 'browser_click'
  | 'browser_fill'
  | 'browser_submit'
  | 'browser_scroll'
  | 'browser_get_cookies'
  | 'browser_set_cookie'
  | 'browser_delete_cookie'
  | 'browser_get_storage'
  | 'browser_set_storage'
  | 'browser_delete_storage'
  | 'browser_clear_storage'
  | 'browser_get_action_plan'
  | 'browser_auto_fill'
  | 'browser_wait'
  | 'browser_get_state'
  | 'browser_list'
  | 'browser_close';
