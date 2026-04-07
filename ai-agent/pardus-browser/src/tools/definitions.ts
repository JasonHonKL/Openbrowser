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
  // ── Extraction tools ────────────────────────────────────────────
  {
    type: 'function',
    function: {
      name: 'browser_extract_text',
      description: 'Extract clean text content from the page, stripping navigation, ads, footers, scripts, and styles. Returns the main readable text. Optionally scope to a CSS selector.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          selector: {
            type: 'string',
            description: 'Optional CSS selector to scope extraction (e.g., "article", "#main-content"). Defaults to the full page body.',
          },
        },
        required: ['instance_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_extract_links',
      description: 'Extract all links from the page with their text, href, and element IDs. Optionally filter by text pattern or domain.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          filter: {
            type: 'string',
            description: 'Optional text pattern to filter links (case-insensitive substring match). E.g., "pdf" to find PDF links.',
          },
          domain: {
            type: 'string',
            description: 'Optional domain to filter links (e.g., "example.com"). Only links matching this domain are returned.',
          },
        },
        required: ['instance_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_find',
      description: 'Search for text within the current page. Returns matching text with surrounding context and element IDs. Like Ctrl+F in a browser.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          query: {
            type: 'string',
            description: 'Text to search for',
          },
          case_sensitive: {
            type: 'boolean',
            description: 'Whether search should be case sensitive (default: false)',
          },
        },
        required: ['instance_id', 'query'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_extract_table',
      description: 'Extract an HTML table as structured data (headers + rows). Returns the first table by default, or a table matching a CSS selector.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          selector: {
            type: 'string',
            description: 'Optional CSS selector for the table. Defaults to the first <table> on the page.',
          },
        },
        required: ['instance_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_extract_metadata',
      description: 'Extract page metadata: JSON-LD structured data, Open Graph tags, meta tags, and title. Useful for understanding page content without reading the full page.',
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
      name: 'browser_screenshot',
      description: 'Take a screenshot of the current page. Returns a base64-encoded image. Use when the semantic tree is insufficient (complex layouts, charts, captchas).',
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
  // ── Interaction tools ───────────────────────────────────────────
  {
    type: 'function',
    function: {
      name: 'browser_select',
      description: 'Select an option in a dropdown <select> element by its value.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          element_id: {
            type: 'string',
            description: 'Element ID of the <select> dropdown (e.g., "#5")',
          },
          value: {
            type: 'string',
            description: 'Value of the option to select',
          },
        },
        required: ['instance_id', 'element_id', 'value'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_press_key',
      description: 'Press a keyboard key. Use for Enter to submit, Escape to close modals, Tab to move between fields, Arrow keys for navigation, etc.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          key: {
            type: 'string',
            description: 'Key to press: "Enter", "Tab", "Escape", "Backspace", "Delete", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown", " ", or a single character.',
          },
        },
        required: ['instance_id', 'key'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_hover',
      description: 'Hover over an element. Triggers mouseover events — useful for revealing dropdown menus, tooltips, preview cards, and hover-triggered content.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          element_id: {
            type: 'string',
            description: 'Element ID to hover over (e.g., "#3")',
          },
        },
        required: ['instance_id', 'element_id'],
      },
    },
  },
  // ── Tab management tools ────────────────────────────────────────
  {
    type: 'function',
    function: {
      name: 'browser_tab_new',
      description: 'Open a new tab in the current browser instance and navigate to a URL. Returns the target ID for the new tab.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          url: {
            type: 'string',
            description: 'URL to open in the new tab',
          },
        },
        required: ['instance_id', 'url'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_tab_switch',
      description: 'Switch to a different tab by its target ID. The semantic tree will update to reflect the active tab.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          target_id: {
            type: 'string',
            description: 'The target ID of the tab to switch to (from browser_tab_new or list)',
          },
        },
        required: ['instance_id', 'target_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_tab_close',
      description: 'Close a tab by its target ID.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: {
            type: 'string',
            description: 'The browser instance ID',
          },
          target_id: {
            type: 'string',
            description: 'The target ID of the tab to close',
          },
        },
        required: ['instance_id', 'target_id'],
      },
    },
  },
  // ── Download/Upload tools ───────────────────────────────────────
  {
    type: 'function',
    function: {
      name: 'browser_download',
      description: 'Download a file from a URL. Returns the file path, size, and MIME type. Useful for saving CSVs, PDFs, images, and documents found during browsing.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          url: { type: 'string', description: 'URL of the file to download' },
          filename: { type: 'string', description: 'Optional filename to save as. Defaults to the URL filename.' },
        },
        required: ['instance_id', 'url'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_upload',
      description: 'Upload a file to an <input type="file"> element.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          element_id: { type: 'string', description: 'Element ID of the file input (e.g., "#3")' },
          file_path: { type: 'string', description: 'Path to the file to upload' },
        },
        required: ['instance_id', 'element_id', 'file_path'],
      },
    },
  },
  // ── Content tools ───────────────────────────────────────────────
  {
    type: 'function',
    function: {
      name: 'browser_pdf_extract',
      description: 'Extract text content from a PDF URL. Returns the text, page count, and any tables or form fields.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          url: { type: 'string', description: 'URL of the PDF file' },
        },
        required: ['instance_id', 'url'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_feed_parse',
      description: 'Parse an RSS or Atom feed URL. Returns feed title, description, and items. Useful for monitoring news sources and blogs.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          url: { type: 'string', description: 'URL of the RSS/Atom feed' },
        },
        required: ['instance_id', 'url'],
      },
    },
  },
  // ── Network control tools ───────────────────────────────────────
  {
    type: 'function',
    function: {
      name: 'browser_network_block',
      description: 'Block network requests by resource type. Speeds up browsing by skipping images, fonts, stylesheets, or media. Pass empty array to clear all blocks.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          resource_types: {
            type: 'array',
            items: { type: 'string' },
            description: 'Resource types to block: "image", "stylesheet", "font", "media", "websocket", "manifest". Pass empty array to clear all blocks.',
          },
        },
        required: ['instance_id', 'resource_types'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_network_log',
      description: 'Get a log of all network requests made by the page. Returns URLs, methods, status codes, MIME types, sizes, and durations.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          filter: { type: 'string', description: 'Optional URL pattern to filter requests (substring match)' },
        },
        required: ['instance_id'],
      },
    },
  },
  // ── Iframe tools ────────────────────────────────────────────────
  {
    type: 'function',
    function: {
      name: 'browser_iframe_enter',
      description: 'Enter an iframe to interact with its content. After entering, subsequent commands operate within the iframe.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          element_id: { type: 'string', description: 'Element ID of the iframe to enter (e.g., "#5")' },
        },
        required: ['instance_id', 'element_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_iframe_exit',
      description: 'Exit the current iframe and return to the parent page context.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
        },
        required: ['instance_id'],
      },
    },
  },
  // ── Page diff tool ──────────────────────────────────────────────
  {
    type: 'function',
    function: {
      name: 'browser_diff',
      description: 'Compare the current page state against the last saved snapshot. Returns changes (added, removed, modified elements). Use to detect what changed after a click, scroll, or wait.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
        },
        required: ['instance_id'],
      },
    },
  },
  // --- OAuth 2.0 / OIDC tools ---
  {
    type: 'function',
    function: {
      name: 'browser_oauth_set_provider',
      description:
        'Register an OAuth 2.0 provider configuration. Must be called before starting an OAuth flow. If only issuer is provided (no explicit endpoints), OIDC auto-discovery is used.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          name: {
            type: 'string',
            description: 'Logical name for this provider (e.g., "google", "github")',
          },
          client_id: { type: 'string', description: 'OAuth client ID' },
          client_secret: {
            type: 'string',
            description: 'Optional client secret (not needed for pure PKCE)',
          },
          issuer: {
            type: 'string',
            description:
              'OIDC issuer URL for auto-discovery (e.g., "https://accounts.google.com")',
          },
          authorization_endpoint: {
            type: 'string',
            description: 'Authorization endpoint URL (required if no issuer)',
          },
          token_endpoint: {
            type: 'string',
            description: 'Token endpoint URL (required if no issuer)',
          },
          scopes: {
            type: 'array',
            items: { type: 'string' },
            description: 'OAuth scopes (e.g., ["openid", "profile", "email"])',
          },
          redirect_uri: {
            type: 'string',
            description:
              'Redirect URI registered with the provider (default: http://localhost:8080/callback)',
          },
        },
        required: ['instance_id', 'name', 'client_id', 'scopes'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_oauth_start',
      description:
        "Start an OAuth 2.0 authorization code flow with PKCE. Navigates to the provider's authorization URL and captures the redirect callback. If login is required, the page content is returned for interaction. Call browser_oauth_complete after login succeeds.",
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          provider: {
            type: 'string',
            description: 'Provider name (from browser_oauth_set_provider)',
          },
          scopes: {
            type: 'array',
            items: { type: 'string' },
            description: 'Override scopes for this flow',
          },
          extra_params: {
            type: 'object',
            description:
              'Extra query parameters for the authorization URL (e.g., {"prompt": "consent"})',
          },
        },
        required: ['instance_id', 'provider'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_oauth_complete',
      description:
        'Complete the OAuth flow by exchanging the captured authorization code for tokens. On success, tokens are stored and automatically injected into future requests to the provider domain.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          provider: { type: 'string', description: 'Provider name' },
          code: {
            type: 'string',
            description: 'Optional: explicit authorization code (if not captured from redirect)',
          },
        },
        required: ['instance_id', 'provider'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'browser_oauth_status',
      description:
        'Get the status of OAuth sessions for the browser instance. Shows active providers, token expiry, and whether auto-injection is active.',
      parameters: {
        type: 'object',
        properties: {
          instance_id: { type: 'string', description: 'The browser instance ID' },
          provider: {
            type: 'string',
            description: 'Optional: get status for a specific provider only',
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
  | 'browser_close'
  | 'browser_extract_text'
  | 'browser_extract_links'
  | 'browser_find'
  | 'browser_extract_table'
  | 'browser_extract_metadata'
  | 'browser_screenshot'
  | 'browser_select'
  | 'browser_press_key'
  | 'browser_hover'
  | 'browser_tab_new'
  | 'browser_tab_switch'
  | 'browser_tab_close'
  | 'browser_download'
  | 'browser_upload'
  | 'browser_pdf_extract'
  | 'browser_feed_parse'
  | 'browser_network_block'
  | 'browser_network_log'
  | 'browser_iframe_enter'
  | 'browser_iframe_exit'
  | 'browser_diff'
  | 'browser_oauth_set_provider'
  | 'browser_oauth_start'
  | 'browser_oauth_complete'
  | 'browser_oauth_status';
