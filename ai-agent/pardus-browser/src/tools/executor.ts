import { BrowserManager, BrowserInstance } from '../core/index.js';
import type { ToolResult } from '../core/types.js';
import { BrowserToolName } from './definitions.js';
import {
  ToolExecutionConfig,
  ToolExecutionResult,
  ParallelExecutionResult,
  groupToolsForParallelExecution,
} from './types.js';

interface ToolCallArgs {
  instance_id?: string;
  url?: string;
  element_id?: string;
  form_element_id?: string;
  value?: string;
  direction?: 'up' | 'down' | 'top' | 'bottom';
  wait_ms?: number;
  interactive_only?: boolean;
  headers?: Record<string, string>;
  proxy?: string;
  timeout?: number;
  // Cookie args
  name?: string;
  domain?: string;
  path?: string;
  expires?: number;
  httpOnly?: boolean;
  secure?: boolean;
  sameSite?: 'Strict' | 'Lax' | 'None';
  // Storage args
  storage_type?: 'localStorage' | 'sessionStorage' | 'both';
  key?: string;
  // Auto-fill args
  fields?: Array<{ key: string; value: string }>;
  // Wait args
  condition?: 'contentLoaded' | 'contentStable' | 'networkIdle' | 'minInteractive' | 'selector';
  selector?: string;
  min_count?: number;
  timeout_ms?: number;
  interval_ms?: number;
  // Extraction args
  filter?: string;
  query?: string;
  case_sensitive?: boolean;
  // Interaction args
  target_id?: string;
  // Download/Upload args
  filename?: string;
  file_path?: string;
  // PDF args
  // Feed args
  // Network args
  resource_types?: string[];
  // OAuth args
  provider?: string;
  issuer?: string;
  client_id?: string;
  client_secret?: string;
  authorization_endpoint?: string;
  token_endpoint?: string;
  redirect_uri?: string;
  scopes?: string[];
  extra_params?: Record<string, string>;
  code?: string;
}

/**
 * Default retry configuration
 */
const DEFAULT_RETRY_CONFIG: Required<ToolExecutionConfig> = {
  timeout: 30000,
  retries: 0,
  retryDelay: 1000,
  maxRetryDelay: 30000,
  retryBackoff: 2,
  retryableErrors: ['TimeoutError', 'NetworkError', 'ConnectionError'],
};

export class ToolExecutor {
  constructor(private browserManager: BrowserManager) {}

  private getInstance(args: Record<string, unknown>): BrowserInstance | null {
    const id = args.instance_id as string | undefined;
    if (!id) return null;
    return this.browserManager.getInstance(id) ?? null;
  }

  private resolveElementId(args: ToolCallArgs): string | undefined {
    const id = args.element_id as string | undefined;
    if (!id) return undefined;
    return id.startsWith('#') ? id.slice(1) : id;
  }

  private resolveFormElementId(args: ToolCallArgs): string | undefined {
    const id = args.form_element_id as string | undefined;
    if (!id) return undefined;
    return id.startsWith('#') ? id.slice(1) : id;
  }

  /**
   * Execute a single browser tool call with retry logic and timeout
   */
  async executeTool(
    name: BrowserToolName,
    args: Record<string, unknown>,
    config?: ToolExecutionConfig
  ): Promise<ToolResult> {
    const startTime = Date.now();
    const mergedConfig = { ...DEFAULT_RETRY_CONFIG, ...config };

    let lastError: Error | undefined;

    for (let attempt = 1; attempt <= mergedConfig.retries + 1; attempt++) {
      try {
        const result = await this.executeToolWithTimeout(
          name,
          args as ToolCallArgs,
          mergedConfig.timeout
        );

        return result;
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        const isRetryable = mergedConfig.retryableErrors.some(
          errPattern => lastError!.message.includes(errPattern) ||
                      lastError!.constructor.name.includes(errPattern)
        );

        if (attempt >= mergedConfig.retries + 1 || !isRetryable) {
          break;
        }

        const delay = Math.min(
          mergedConfig.retryDelay * Math.pow(mergedConfig.retryBackoff, attempt - 1),
          mergedConfig.maxRetryDelay
        );

        console.log(`[Retry] ${name} attempt ${attempt + 1}/${mergedConfig.retries + 1} after ${delay}ms`);
        await this.sleep(delay);
      }
    }

    return {
      success: false,
      content: '',
      error: lastError?.message || 'Tool execution failed',
    };
  }

  /**
   * Execute tool with timeout
   */
  private async executeToolWithTimeout(
    name: BrowserToolName,
    args: ToolCallArgs,
    timeout: number
  ): Promise<ToolResult> {
    return new Promise<ToolResult>((resolve, reject) => {
      const timeoutId = setTimeout(
        () => reject(new Error(`TimeoutError: Tool ${name} timed out after ${timeout}ms`)),
        timeout
      );
      this.executeToolInternal(name, args)
        .then(result => { clearTimeout(timeoutId); resolve(result); })
        .catch(error => { clearTimeout(timeoutId); reject(error); });
    });
  }

  /**
   * Execute multiple tool calls with parallel execution where safe
   */
  async executeTools(
    tools: Array<{
      toolCallId: string;
      name: BrowserToolName;
      args: Record<string, unknown>;
      config?: ToolExecutionConfig;
    }>,
    options?: {
      parallel?: boolean;
      continueOnError?: boolean;
    }
  ): Promise<ParallelExecutionResult> {
    const results: ToolExecutionResult[] = [];

    if (!options?.parallel) {
      for (const tool of tools) {
        const result = await this.executeToolWithTracking(
          tool.toolCallId,
          tool.name,
          tool.args,
          tool.config
        );
        results.push(result);

        if (!result.success && !options?.continueOnError) {
          break;
        }
      }
    } else {
      const groups = groupToolsForParallelExecution(tools);

      for (const group of groups) {
        const groupResults = await Promise.all(
          group.tools.map(tool =>
            this.executeToolWithTracking(
              tool.toolCallId!,
              tool.name as BrowserToolName,
              tool.args,
              tool.config
            )
          )
        );

        results.push(...groupResults);

        const hasFailure = groupResults.some(r => !r.success);
        if (hasFailure) {
          if (group.failureStrategy === 'abort') {
            break;
          } else if (group.failureStrategy === 'retry-all') {
            const retryResults = await Promise.all(
              group.tools.map(tool =>
                this.executeToolWithTracking(
                  tool.toolCallId!,
                  tool.name as BrowserToolName,
                  tool.args,
                  { ...tool.config, retries: (tool.config?.retries ?? 0) + 1 }
                )
              )
            );
            results.splice(results.length - groupResults.length, groupResults.length, ...retryResults);
          }
        }
      }
    }

    const succeeded = results.filter(r => r.success);
    const failed = results.filter(r => !r.success);

    return {
      results,
      allSucceeded: failed.length === 0,
      anySucceeded: succeeded.length > 0,
      failedCount: failed.length,
      succeededCount: succeeded.length,
    };
  }

  /**
   * Execute a single tool and track execution details
   */
  private async executeToolWithTracking(
    toolCallId: string,
    name: BrowserToolName,
    args: Record<string, unknown>,
    config?: ToolExecutionConfig
  ): Promise<ToolExecutionResult> {
    const startTime = Date.now();
    const mergedConfig = { ...DEFAULT_RETRY_CONFIG, ...config };

    let lastError: string | undefined;
    let lastContent: string | undefined;
    let attempts = 0;

    for (let attempt = 1; attempt <= mergedConfig.retries + 1; attempt++) {
      attempts = attempt;
      try {
        const result = await this.executeToolWithTimeout(
          name,
          args as ToolCallArgs,
          mergedConfig.timeout
        );

        return {
          toolCallId,
          name,
          args,
          success: result.success,
          content: result.content,
          error: result.error,
          durationMs: Date.now() - startTime,
          attempts,
        };
      } catch (error) {
        lastError = error instanceof Error ? error.message : String(error);
        lastContent = '';

        const isRetryable = mergedConfig.retryableErrors.some(
          errPattern => lastError!.includes(errPattern)
        );

        if (attempt >= mergedConfig.retries + 1 || !isRetryable) {
          break;
        }

        const delay = Math.min(
          mergedConfig.retryDelay * Math.pow(mergedConfig.retryBackoff, attempt - 1),
          mergedConfig.maxRetryDelay
        );

        await this.sleep(delay);
      }
    }

    return {
      toolCallId,
      name,
      args,
      success: false,
      content: lastContent,
      error: lastError,
      durationMs: Date.now() - startTime,
      attempts,
    };
  }

  /**
   * Internal tool execution without timeout/retry logic
   */
  private async executeToolInternal(
    name: BrowserToolName,
    args: ToolCallArgs
  ): Promise<ToolResult> {
    switch (name) {
      case 'browser_new':
        return this.handleNew(args);
      case 'browser_navigate':
        return this.handleNavigate(args);
      case 'browser_click':
        return this.handleClick(args);
      case 'browser_fill':
        return this.handleFill(args);
      case 'browser_submit':
        return this.handleSubmit(args);
      case 'browser_scroll':
        return this.handleScroll(args);
      case 'browser_get_cookies':
        return this.handleGetCookies(args);
      case 'browser_set_cookie':
        return this.handleSetCookie(args);
      case 'browser_delete_cookie':
        return this.handleDeleteCookie(args);
      case 'browser_get_storage':
        return this.handleGetStorage(args);
      case 'browser_set_storage':
        return this.handleSetStorage(args);
      case 'browser_delete_storage':
        return this.handleDeleteStorage(args);
      case 'browser_clear_storage':
        return this.handleClearStorage(args);
      case 'browser_get_action_plan':
        return this.handleGetActionPlan(args);
      case 'browser_auto_fill':
        return this.handleAutoFill(args);
      case 'browser_wait':
        return this.handleWait(args);
      case 'browser_close':
        return this.handleClose(args);
      case 'browser_list':
        return this.handleList();
      case 'browser_get_state':
        return this.handleGetState(args);
      case 'browser_extract_text':
        return this.handleExtractText(args);
      case 'browser_extract_links':
        return this.handleExtractLinks(args);
      case 'browser_find':
        return this.handleFind(args);
      case 'browser_extract_table':
        return this.handleExtractTable(args);
      case 'browser_extract_metadata':
        return this.handleExtractMetadata(args);
      case 'browser_screenshot':
        return this.handleScreenshot(args);
      case 'browser_select':
        return this.handleSelect(args);
      case 'browser_press_key':
        return this.handlePressKey(args);
      case 'browser_hover':
        return this.handleHover(args);
      case 'browser_tab_new':
        return this.handleTabNew(args);
      case 'browser_tab_switch':
        return this.handleTabSwitch(args);
      case 'browser_tab_close':
        return this.handleTabClose(args);
      case 'browser_download':
        return this.handleDownload(args);
      case 'browser_upload':
        return this.handleUpload(args);
      case 'browser_pdf_extract':
        return this.handlePdfExtract(args);
      case 'browser_feed_parse':
        return this.handleFeedParse(args);
      case 'browser_network_block':
        return this.handleNetworkBlock(args);
      case 'browser_network_log':
        return this.handleNetworkLog(args);
      case 'browser_iframe_enter':
        return this.handleIframeEnter(args);
      case 'browser_iframe_exit':
        return this.handleIframeExit(args);
      case 'browser_diff':
        return this.handleDiff(args);
      case 'browser_oauth_set_provider':
        return this.handleOAuthSetProvider(args);
      case 'browser_oauth_start':
        return this.handleOAuthStart(args);
      case 'browser_oauth_complete':
        return this.handleOAuthComplete(args);
      case 'browser_oauth_status':
        return this.handleOAuthStatus(args);
      default:
        return {
          success: false,
          content: '',
          error: `Unknown tool: ${name}`,
        };
    }
  }

  private requireInstance(args: ToolCallArgs): { instance: BrowserInstance; error?: undefined } | { instance?: undefined; error: ToolResult } {
    if (!args.instance_id) {
      return { error: { success: false, content: '', error: 'Missing instance_id' } };
    }
    const instance = this.browserManager.getInstance(args.instance_id);
    if (!instance) {
      return { error: { success: false, content: '', error: `Browser instance "${args.instance_id}" not found` } };
    }
    return { instance };
  }

  private async handleNew(args: ToolCallArgs): Promise<ToolResult> {
    try {
      const instance = await this.browserManager.createInstance({
        id: args.instance_id,
        proxy: args.proxy,
        timeout: args.timeout,
      });

      return {
        success: true,
        content: `Browser instance created: ${instance.id}\nPort: ${instance.port}\nStatus: connected`,
      };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleNavigate(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.url) {
      return { success: false, content: '', error: 'Missing url' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.navigate(args.url, {
        waitMs: args.wait_ms,
        interactiveOnly: args.interactive_only,
        headers: args.headers,
      });

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Navigation failed',
        };
      }

      const content = `## Navigation Result\n\n` +
        `- **URL**: ${result.url}\n` +
        `- **Title**: ${result.title || 'N/A'}\n` +
        `- **Stats**: ${result.stats.landmarks} landmarks, ${result.stats.links} links, ${result.stats.headings} headings, ${result.stats.actions} actions, ${result.stats.forms} forms\n\n` +
        `---\n\n` +
        `## Page Content\n\n` +
        result.markdown;

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleClick(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.element_id) {
      return { success: false, content: '', error: 'Missing element_id' };
    }

    const elementId = this.resolveElementId(args);

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.click(elementId!);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Click failed',
        };
      }

      let content = `## Click Result\n\n` +
        `- **Element**: #${elementId}\n` +
        `- **Navigated**: ${result.navigated ? 'Yes' : 'No'}\n`;

      if (result.navigated && result.url) {
        content += `- **New URL**: ${result.url}\n`;
      }

      if (result.markdown) {
        content += `\n---\n\n## Page Content\n\n${result.markdown}`;
      }

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleFill(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.element_id) {
      return { success: false, content: '', error: 'Missing element_id' };
    }
    if (args.value === undefined) {
      return { success: false, content: '', error: 'Missing value' };
    }

    const elementId = this.resolveElementId(args);

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.fill(elementId!, args.value);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Fill failed',
        };
      }

      return {
        success: true,
        content: `Filled #${elementId} with: ${args.value}`,
      };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleSubmit(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.submit(this.resolveFormElementId(args));

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Submit failed',
        };
      }

      let content = `## Submit Result\n\n` +
        `- **Navigated**: ${result.navigated ? 'Yes' : 'No'}\n`;

      if (result.navigated && result.url) {
        content += `- **New URL**: ${result.url}\n`;
      }

      if (result.markdown) {
        content += `\n---\n\n## Page Content\n\n${result.markdown}`;
      }

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleScroll(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.direction) {
      return { success: false, content: '', error: 'Missing direction' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.scroll(args.direction);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Scroll failed',
        };
      }

      let content = `## Scroll Result\n\n- **Direction**: ${args.direction}\n`;

      if (result.markdown) {
        content += `\n---\n\n## Page Content\n\n${result.markdown}`;
      }

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // Cookie handlers
  private async handleGetCookies(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.getCookies(args.url);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Failed to get cookies',
        };
      }

      const cookieList = result.cookies.map(c => 
        `- ${c.name}=${c.value.substring(0, 50)}${c.value.length > 50 ? '...' : ''} (domain: ${c.domain}, path: ${c.path})`
      ).join('\n');

      const content = `## Cookies (${result.cookies.length})\n\n${cookieList || 'No cookies found'}`;

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleSetCookie(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.name) {
      return { success: false, content: '', error: 'Missing cookie name' };
    }
    if (args.value === undefined) {
      return { success: false, content: '', error: 'Missing cookie value' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.setCookie(args.name, args.value, {
        url: args.url,
        domain: args.domain,
        path: args.path,
        expires: args.expires,
        httpOnly: args.httpOnly,
        secure: args.secure,
        sameSite: args.sameSite,
      });

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Failed to set cookie',
        };
      }

      return {
        success: true,
        content: `Cookie "${args.name}" set successfully`,
      };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleDeleteCookie(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.name) {
      return { success: false, content: '', error: 'Missing cookie name' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.deleteCookie(args.name, args.url);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Failed to delete cookie',
        };
      }

      return {
        success: true,
        content: `Cookie "${args.name}" deleted successfully`,
      };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // Storage handlers
  private async handleGetStorage(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.storage_type) {
      return { success: false, content: '', error: 'Missing storage_type' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.getStorage(args.storage_type as 'localStorage' | 'sessionStorage', args.key);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Failed to get storage',
        };
      }

      const storageList = result.items.map(item => 
        `- ${item.key}: ${item.value.substring(0, 100)}${item.value.length > 100 ? '...' : ''}`
      ).join('\n');

      const content = `## ${args.storage_type} (${result.items.length} items)\n\n${storageList || 'No items found'}`;

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleSetStorage(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.storage_type) {
      return { success: false, content: '', error: 'Missing storage_type' };
    }
    if (!args.key) {
      return { success: false, content: '', error: 'Missing key' };
    }
    if (args.value === undefined) {
      return { success: false, content: '', error: 'Missing value' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.setStorage(args.storage_type as 'localStorage' | 'sessionStorage', args.key, args.value);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Failed to set storage',
        };
      }

      return {
        success: true,
        content: `Set ${args.key} in ${args.storage_type}`,
      };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleDeleteStorage(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.storage_type) {
      return { success: false, content: '', error: 'Missing storage_type' };
    }
    if (!args.key) {
      return { success: false, content: '', error: 'Missing key' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.deleteStorage(args.storage_type as 'localStorage' | 'sessionStorage', args.key);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Failed to delete storage',
        };
      }

      return {
        success: true,
        content: `Deleted ${args.key} from ${args.storage_type}`,
      };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleClearStorage(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.storage_type) {
      return { success: false, content: '', error: 'Missing storage_type' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.clearStorage(args.storage_type);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Failed to clear storage',
        };
      }

      return {
        success: true,
        content: `Cleared ${args.storage_type}`,
      };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleGetActionPlan(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.getActionPlan();

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Failed to get action plan',
        };
      }

      const plan = result.actionPlan;
      if (!plan) {
        return { success: true, content: '## Action Plan\n\nNo suggestions for current page.' };
      }

      const pageTypeLabel = plan.page_type
        .replace(/([A-Z])/g, ' $1')
        .trim()
        .replace(/^./, (c) => c.toUpperCase());

      let content = `## Action Plan\n\n` +
        `- **URL**: ${plan.url}\n` +
        `- **Page Type**: ${pageTypeLabel}\n` +
        `- **Interactive Elements**: ${plan.interactive_count}\n` +
        `- **Has Forms**: ${plan.has_forms ? 'Yes' : 'No'}\n` +
        `- **Has Pagination**: ${plan.has_pagination ? 'Yes' : 'No'}\n`;

      if (plan.suggestions.length > 0) {
        content += `\n### Suggested Actions\n\n`;
        for (const s of plan.suggestions) {
          const pct = Math.round(s.confidence * 100);
          content += `- **${s.action_type}** (${pct}%): ${s.reason}`;
          if (s.label) content += ` — ${s.label}`;
          if (s.element_id) content += ` [#${s.element_id}]`;
          if (s.selector) content += ` (${s.selector})`;
          content += '\n';
        }
      } else {
        content += '\nNo suggested actions for this page.';
      }

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleAutoFill(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.fields || args.fields.length === 0) {
      return { success: false, content: '', error: 'Missing fields (array of {key, value} pairs)' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.autoFill(args.fields);

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Auto-fill failed',
        };
      }

      let content = '## Auto-Fill Result\n\n';

      if (result.filledFields && result.filledFields.length > 0) {
        content += `### Filled Fields (${result.filledFields.length})\n\n`;
        for (const f of result.filledFields) {
          content += `- **${f.field_name}** = "${f.value}" (matched by ${f.matched_by})\n`;
        }
      }

      if (result.unmatchedFields && result.unmatchedFields.length > 0) {
        content += `\n### Unmatched Fields (${result.unmatchedFields.length})\n\n`;
        for (const f of result.unmatchedFields) {
          const req = f.required ? ' [required]' : '';
          content += `- ${f.field_type || 'unknown'}${req}`;
          if (f.field_name) content += `: "${f.field_name}"`;
          if (f.label) content += ` (label: "${f.label}")`;
          if (f.placeholder) content += ` (placeholder: "${f.placeholder}")`;
          content += '\n';
        }
      }

      if ((!result.filledFields || result.filledFields.length === 0) &&
          (!result.unmatchedFields || result.unmatchedFields.length === 0)) {
        content += 'No form fields found on the current page.';
      }

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleWait(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.condition) {
      return { success: false, content: '', error: 'Missing condition (contentLoaded, contentStable, networkIdle, minInteractive, or selector)' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const validConditions = ['contentLoaded', 'contentStable', 'networkIdle', 'minInteractive', 'selector'] as const;
      const condition = args.condition as typeof validConditions[number];
      if (!validConditions.includes(condition)) {
        return { success: false, content: '', error: `Invalid condition: ${args.condition}` };
      }

      if (condition === 'selector' && !args.selector) {
        return { success: false, content: '', error: 'selector is required when condition is "selector"' };
      }

      const result = await inst.instance.wait(condition, {
        selector: args.selector,
        minCount: args.min_count,
        timeoutMs: args.timeout_ms,
        intervalMs: args.interval_ms,
      });

      if (!result.success) {
        return {
          success: false,
          content: '',
          error: result.error || 'Wait failed',
        };
      }

      const status = result.satisfied ? 'Satisfied' : 'Not satisfied';
      const reason = result.reason ? ` (${result.reason})` : '';
      const content = `## Wait Result\n\n` +
        `- **Condition**: ${result.condition}\n` +
        `- **Status**: ${status}${reason}\n` +
        `- **Timeout**: ${args.timeout_ms ?? 10000}ms`;

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async handleClose(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    try {
      await this.browserManager.closeInstance(args.instance_id);
      return {
        success: true,
        content: `Browser instance "${args.instance_id}" closed`,
      };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private handleList(): ToolResult {
    const instances = this.browserManager.listInstances();
    
    if (instances.length === 0) {
      return {
        success: true,
        content: 'No active browser instances.',
      };
    }

    const lines = instances.map((inst) => {
      const status = inst.connected ? 'connected' : 'disconnected';
      const url = inst.url ? ` - ${inst.url}` : '';
      return `- **${inst.id}** (${status}, port ${inst.port})${url}`;
    });

    return {
      success: true,
      content: `## Active Browser Instances (${instances.length})\n\n${lines.join('\n')}`,
    };
  }

  private async handleGetState(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const state = await inst.instance.getCurrentState();
      
      const content = `## Current State\n\n` +
        `- **URL**: ${state.url}\n` +
        `- **Title**: ${state.title || 'N/A'}\n\n` +
        `---\n\n` +
        `## Page Content\n\n` +
        state.markdown;

      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        content: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Extraction handlers ────────────────────────────────────────

  private async handleExtractText(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.extractText(args.selector);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Text extraction failed' };
      }

      const content = `## Extracted Text\n\n` +
        `- **Word Count**: ${result.word_count}\n` +
        `- **Scope**: ${args.selector || 'full page'}\n\n` +
        `---\n\n${result.text}`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleExtractLinks(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.extractLinks(args.filter, args.domain);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Link extraction failed' };
      }

      const filterNote = args.filter ? ` (filtered: "${args.filter}")` : '';
      const domainNote = args.domain ? ` (domain: ${args.domain})` : '';
      const linkLines = result.links.map((l, i) => {
        const id = l.element_id ? ` [#${l.element_id}]` : '';
        return `${i + 1}. [${l.text}](${l.href})${id}`;
      }).join('\n');

      const content = `## Links (${result.count})${filterNote}${domainNote}\n\n${linkLines || 'No links found.'}`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleFind(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.query) {
      return { success: false, content: '', error: 'Missing query' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.find(args.query, args.case_sensitive);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Find failed' };
      }

      const matchLines = result.matches.map((m, i) => {
        const id = m.element_id ? ` [element #${m.element_id}]` : '';
        return `${i + 1}. "${m.text}"${id}\n   ...${m.context}...`;
      }).join('\n\n');

      const content = `## Search Results\n\n` +
        `- **Query**: "${args.query}"\n` +
        `- **Matches**: ${result.count}\n\n` +
        `---\n\n${matchLines || 'No matches found.'}`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleExtractTable(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.extractTable(args.selector);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Table extraction failed' };
      }

      const headerLine = `| ${result.headers.join(' | ')} |`;
      const separatorLine = `| ${result.headers.map(() => '---').join(' | ')} |`;
      const dataLines = result.rows.map(row => `| ${row.join(' | ')} |`).join('\n');

      const content = `## Table (${result.row_count} rows)\n\n` +
        `${headerLine}\n${separatorLine}\n${dataLines}`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleExtractMetadata(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.extractMetadata();

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Metadata extraction failed' };
      }

      let content = `## Page Metadata\n\n` +
        `- **Title**: ${result.title}\n`;

      if (result.description) {
        content += `- **Description**: ${result.description}\n`;
      }

      if (Object.keys(result.open_graph).length > 0) {
        content += `\n### Open Graph\n\n`;
        for (const [key, val] of Object.entries(result.open_graph)) {
          content += `- **og:${key}**: ${val}\n`;
        }
      }

      if (result.json_ld.length > 0) {
        content += `\n### JSON-LD\n\n\`\`\`json\n${JSON.stringify(result.json_ld, null, 2)}\n\`\`\`\n`;
      }

      if (Object.keys(result.meta).length > 0) {
        content += `\n### Meta Tags\n\n`;
        for (const [key, val] of Object.entries(result.meta)) {
          content += `- **${key}**: ${val}\n`;
        }
      }

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleScreenshot(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.screenshot();

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Screenshot failed' };
      }

      const content = `## Screenshot\n\n` +
        `- **MIME Type**: ${result.mime_type}\n` +
        `- **Data Length**: ${result.data.length} bytes (base64)\n\n` +
        `![Screenshot](data:${result.mime_type};base64,${result.data.substring(0, 100)}...)`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  // ── Interaction handlers ───────────────────────────────────────

  private async handleSelect(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.element_id) {
      return { success: false, content: '', error: 'Missing element_id' };
    }
    if (!args.value) {
      return { success: false, content: '', error: 'Missing value' };
    }

    const elementId = this.resolveElementId(args);

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.selectOption(elementId!, args.value);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Select failed' };
      }

      return {
        success: true,
        content: `Selected "${result.selected_value}" in dropdown #${elementId}`,
      };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handlePressKey(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.key) {
      return { success: false, content: '', error: 'Missing key' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.pressKey(args.key);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Press key failed' };
      }

      return {
        success: true,
        content: `Pressed key: ${args.key}`,
      };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleHover(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.element_id) {
      return { success: false, content: '', error: 'Missing element_id' };
    }

    const elementId = this.resolveElementId(args);

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.hover(elementId!);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Hover failed' };
      }

      return {
        success: true,
        content: `Hovered over element #${elementId}`,
      };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  // ── Tab management handlers ────────────────────────────────────

  private async handleTabNew(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.url) {
      return { success: false, content: '', error: 'Missing url' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.newTab(args.url);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Failed to create tab' };
      }

      return {
        success: true,
        content: `## New Tab\n\n- **Target ID**: ${result.target_id}\n- **URL**: ${args.url}`,
      };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleTabSwitch(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.target_id) {
      return { success: false, content: '', error: 'Missing target_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.switchTab(args.target_id);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Failed to switch tab' };
      }

      return {
        success: true,
        content: `Switched to tab ${args.target_id}`,
      };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleTabClose(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) {
      return { success: false, content: '', error: 'Missing instance_id' };
    }
    if (!args.target_id) {
      return { success: false, content: '', error: 'Missing target_id' };
    }

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.closeTab(args.target_id);

      if (!result.success) {
        return { success: false, content: '', error: result.error || 'Failed to close tab' };
      }

      return {
        success: true,
        content: `Closed tab ${args.target_id}`,
      };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  // ── Download/Upload handlers ────────────────────────────────────

  private async handleDownload(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };
    if (!args.url) return { success: false, content: '', error: 'Missing url' };

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.download(args.url, args.filename);
      if (!result.success) return { success: false, content: '', error: result.error || 'Download failed' };

      const content = `## Download Complete\n\n` +
        `- **URL**: ${args.url}\n` +
        `- **Path**: ${result.path}\n` +
        `- **Size**: ${result.size_bytes} bytes (${(result.size_bytes / 1024).toFixed(1)} KB)\n` +
        (result.mime_type ? `- **MIME Type**: ${result.mime_type}\n` : '');

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleUpload(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };
    if (!args.element_id) return { success: false, content: '', error: 'Missing element_id' };
    if (!args.file_path) return { success: false, content: '', error: 'Missing file_path' };

    const elementId = this.resolveElementId(args);

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.upload(elementId!, args.file_path);
      if (!result.success) return { success: false, content: '', error: result.error || 'Upload failed' };

      return { success: true, content: `Uploaded "${args.file_path}" to element #${elementId}` };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  // ── Content handlers ────────────────────────────────────────────

  private async handlePdfExtract(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };
    if (!args.url) return { success: false, content: '', error: 'Missing url' };

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.pdfExtract(args.url);
      if (!result.success) return { success: false, content: '', error: result.error || 'PDF extraction failed' };

      let content = `## PDF Extract\n\n` +
        `- **URL**: ${args.url}\n` +
        `- **Pages**: ${result.page_count}\n`;

      if (result.forms && result.forms.length > 0) {
        content += `- **Form Fields**: ${result.forms.length}\n`;
      }

      content += `\n---\n\n${result.text.substring(0, 8000)}`;
      if (result.text.length > 8000) content += '\n\n... [truncated]';

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleFeedParse(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };
    if (!args.url) return { success: false, content: '', error: 'Missing url' };

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.feedParse(args.url);
      if (!result.success) return { success: false, content: '', error: result.error || 'Feed parse failed' };

      let content = `## ${result.feed_type.toUpperCase()} Feed\n\n` +
        `- **Title**: ${result.title}\n` +
        (result.description ? `- **Description**: ${result.description}\n` : '') +
        `- **Items**: ${result.item_count}\n\n`;

      for (const item of result.items.slice(0, 20)) {
        content += `### ${item.title}\n`;
        content += `- **Link**: ${item.link}\n`;
        if (item.pub_date) content += `- **Date**: ${item.pub_date}\n`;
        if (item.author) content += `- **Author**: ${item.author}\n`;
        if (item.description) content += `- **Summary**: ${item.description.substring(0, 200)}${item.description.length > 200 ? '...' : ''}\n`;
        content += '\n';
      }

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  // ── Network control handlers ────────────────────────────────────

  private async handleNetworkBlock(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };
    if (!args.resource_types) return { success: false, content: '', error: 'Missing resource_types' };

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.networkBlock(args.resource_types);
      if (!result.success) return { success: false, content: '', error: result.error || 'Network block failed' };

      const content = args.resource_types.length === 0
        ? '## Network Block Cleared\n\nAll resource blocks removed.'
        : `## Network Block Set\n\nBlocked resource types: ${result.blocked_types.join(', ')}`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleNetworkLog(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.networkLog(args.filter);
      if (!result.success) return { success: false, content: '', error: result.error || 'Network log failed' };

      let content = `## Network Log (${result.count} requests)\n\n`;

      for (const req of result.requests.slice(0, 30)) {
        content += `- **${req.method}** ${req.status} ${req.url.substring(0, 100)}${req.url.length > 100 ? '...' : ''} (${req.size_bytes} bytes, ${req.duration_ms}ms)\n`;
      }

      if (result.count > 30) content += `\n... and ${result.count - 30} more requests`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  // ── Iframe handlers ─────────────────────────────────────────────

  private async handleIframeEnter(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };
    if (!args.element_id) return { success: false, content: '', error: 'Missing element_id' };

    const elementId = this.resolveElementId(args);

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.iframeEnter(elementId!);
      if (!result.success) return { success: false, content: '', error: result.error || 'Failed to enter iframe' };

      return { success: true, content: `Entered iframe #${elementId}. Subsequent commands now operate within the iframe.` };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleIframeExit(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.iframeExit();
      if (!result.success) return { success: false, content: '', error: result.error || 'Failed to exit iframe' };

      return { success: true, content: 'Returned to parent page context.' };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  // ── Page diff handler ───────────────────────────────────────────

  private async handleDiff(args: ToolCallArgs): Promise<ToolResult> {
    if (!args.instance_id) return { success: false, content: '', error: 'Missing instance_id' };

    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const result = await inst.instance.diff();
      if (!result.success) return { success: false, content: '', error: result.error || 'Diff failed' };

      let content = `## Page Diff (${result.change_count} changes)\n\n`;

      if (result.summary) content += `${result.summary}\n\n`;

      for (const change of result.changes.slice(0, 30)) {
        const icon = change.type === 'added' ? '+' : change.type === 'removed' ? '-' : '~';
        content += `${icon} **[${change.type}]** ${change.selector}\n`;
        if (change.type === 'added' && change.text) content += `  "${change.text.substring(0, 100)}"\n`;
        if (change.type === 'removed' && change.text) content += `  "${change.text.substring(0, 100)}"\n`;
        if (change.type === 'modified' && change.old_text && change.new_text) {
          content += `  was: "${change.old_text.substring(0, 80)}"\n`;
          content += `  now: "${change.new_text.substring(0, 80)}"\n`;
        }
      }

      if (result.change_count > 30) content += `\n... and ${result.change_count - 30} more changes`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  // ---------------------------------------------------------------------------
  // OAuth 2.0 / OIDC handlers
  // ---------------------------------------------------------------------------

  private async handleOAuthSetProvider(args: ToolCallArgs): Promise<ToolResult> {
    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const data = await inst.instance.sendCommand('OAuth.setProvider', {
        name: args.provider || args.name,
        client_id: args.client_id,
        client_secret: args.client_secret,
        issuer: args.issuer,
        authorization_endpoint: args.authorization_endpoint,
        token_endpoint: args.token_endpoint,
        scopes: args.scopes,
        redirect_uri: args.redirect_uri,
      }) as Record<string, unknown>;

      return {
        success: true,
        content: `OAuth provider "${data?.provider}" registered successfully.${data?.discovered ? ' Endpoints auto-discovered via OIDC.' : ''}`,
      };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleOAuthStart(args: ToolCallArgs): Promise<ToolResult> {
    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const navData = await inst.instance.sendCommand('OAuth.navigateForAuth', {
        provider: args.provider,
      }) as Record<string, unknown>;
      const status = navData?.status as string;

      if (status === 'callback_captured') {
        return {
          success: true,
          content: `OAuth callback captured. Authorization code received. Call browser_oauth_complete to exchange for tokens.`,
        };
      }

      return {
        success: true,
        content: `OAuth flow started. Landed on login/consent page at: ${navData?.url}.\n\nUse browser_fill and browser_submit to log in, then call browser_oauth_complete to finish the flow.`,
      };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleOAuthComplete(args: ToolCallArgs): Promise<ToolResult> {
    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const data = await inst.instance.sendCommand('OAuth.completeFlow', {
        provider: args.provider,
        code: args.code,
      }) as Record<string, unknown>;

      let content = `OAuth flow completed successfully.\n`;
      content += `- Has refresh token: ${data?.hasRefreshToken ? 'yes' : 'no'}\n`;
      if (data?.expiresAt) content += `- Expires at: ${new Date((data.expiresAt as number) * 1000).toISOString()}\n`;
      if (data?.scopes) content += `- Scopes: ${(data.scopes as string[]).join(', ')}\n`;
      if (data?.idTokenClaims) {
        const claims = data.idTokenClaims as Record<string, unknown>;
        content += `- User: ${claims.email || claims.sub}\n`;
      }
      content += `\nTokens will be automatically injected into future requests to this provider's domain.`;

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private async handleOAuthStatus(args: ToolCallArgs): Promise<ToolResult> {
    const inst = this.requireInstance(args);
    if (inst.error) return inst.error;

    try {
      const data = await inst.instance.sendCommand('OAuth.listSessions', {}) as Record<string, unknown>;
      const sessions = (data?.sessions || []) as Array<Record<string, unknown>>;

      if (sessions.length === 0) {
        return { success: true, content: 'No OAuth sessions registered. Use browser_oauth_set_provider to register one.' };
      }

      let content = `## OAuth Sessions (${sessions.length})\n\n`;
      for (const session of sessions) {
        const icon = session.status === 'active' ? '✓' : session.status === 'authorization_pending' ? '⏳' : '○';
        content += `${icon} **${session.provider}** — ${session.status}\n`;
        content += `  Access token: ${session.has_access_token ? 'yes' : 'no'} | Refresh token: ${session.has_refresh_token ? 'yes' : 'no'}\n`;
        if (session.expires_at) {
          const expired = (session.expires_at as number) * 1000 < Date.now();
          content += `  Expires: ${new Date((session.expires_at as number) * 1000).toISOString()} ${expired ? '(EXPIRED)' : ''}\n`;
        }
        if (session.scopes) content += `  Scopes: ${session.scopes}\n`;
        content += '\n';
      }

      return { success: true, content };
    } catch (error) {
      return { success: false, content: '', error: error instanceof Error ? error.message : String(error) };
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}
