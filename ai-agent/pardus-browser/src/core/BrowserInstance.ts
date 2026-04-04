import { spawn, ChildProcess } from 'node:child_process';
import WebSocket from 'ws';
import { EventEmitter } from 'node:events';
import {
  BrowserNavigateResult,
  BrowserClickResult,
  BrowserFillResult,
  BrowserSubmitResult,
  BrowserScrollResult,
  Cookie,
  BrowserGetCookiesResult,
  BrowserSetCookieResult,
  BrowserDeleteCookieResult,
  StorageItem,
  BrowserGetStorageResult,
  BrowserSetStorageResult,
  BrowserDeleteStorageResult,
  BrowserClearStorageResult,
  BrowserGetActionPlanResult,
  BrowserAutoFillResult,
  BrowserWaitResult,
  BrowserExtractTextResult,
  LinkItem,
  BrowserExtractLinksResult,
  TextMatch,
  BrowserFindResult,
  BrowserExtractTableResult,
  BrowserExtractMetadataResult,
  BrowserScreenshotResult,
  BrowserSelectResult,
  BrowserPressKeyResult,
  BrowserHoverResult,
  BrowserTabNewResult,
  BrowserTabSwitchResult,
  BrowserTabCloseResult,
  BrowserTabListResult,
  TabInfo,
  BrowserDownloadResult,
  BrowserUploadResult,
  BrowserPdfExtractResult,
  FeedItem,
  BrowserFeedParseResult,
  BrowserNetworkBlockResult,
  BrowserNetworkLogResult,
  BrowserIframeEnterResult,
  BrowserIframeExitResult,
  PageDiffChange,
  BrowserDiffResult,
} from './types.js';

interface CDPResponse {
  id: number;
  result?: unknown;
  error?: { code: number; message: string };
}

interface CDPEvent {
  method: string;
  params?: Record<string, unknown>;
}

export class BrowserInstance extends EventEmitter {
  private process: ChildProcess | null = null;
  private ws: WebSocket | null = null;
  private messageId = 0;
  private pendingRequests = new Map<number, { resolve: (value: unknown) => void; reject: (reason: Error) => void }>();
  private requestTimeout = 30000; // 30 second default timeout
  private navigateTimeout = 60000; // 60 seconds for navigation
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectBaseDelay = 500; // ms
  private isReconnecting = false;
  private intentionallyClosed = false;

  public readonly id: string;
  public readonly port: number;
  public currentUrl?: string;
  private connected = false;

  constructor(id: string, port: number) {
    super();
    this.id = id;
    this.port = port;
  }

  async spawn(proxy?: string): Promise<void> {
    const args = ['serve', '--port', String(this.port)];
    if (proxy) {
      args.push('--proxy', proxy);
    }

    return new Promise((resolve, reject) => {
      this.process = spawn('pardus-browser', args, {
        stdio: ['ignore', 'pipe', 'pipe'],
        env: { ...process.env },
      });

      let stdout = '';
      let stderr = '';
      let connected = false;

      const timeout = setTimeout(() => {
        this.kill();
        reject(new Error('Browser spawn timeout after 10s'));
      }, 10000);

      this.process.stdout?.on('data', (data: Buffer) => {
        stdout += data.toString();
        if (!connected && (stdout.includes('9222') || stdout.includes('CDP') || stdout.includes('WebSocket'))) {
          connected = true;
          clearTimeout(timeout);
          setTimeout(() => this.connectWebSocket().then(resolve).catch(reject), 500);
        }
      });

      this.process.stderr?.on('data', (data: Buffer) => {
        stderr += data.toString();
      });

      this.process.on('error', (err) => {
        clearTimeout(timeout);
        reject(new Error(`Failed to spawn browser: ${err.message}`));
      });

      this.process.on('exit', (code) => {
        if (!connected && code !== null) {
          clearTimeout(timeout);
          reject(new Error(`Browser process exited with code ${code}: ${stderr}`));
        }
        this.emit('exit', code);
      });
    });
  }

  private async connectWebSocket(): Promise<void> {
    const wsUrl = `ws://127.0.0.1:${this.port}`;
    
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error('WebSocket connection timeout'));
      }, 5000);

      this.ws = new WebSocket(wsUrl);

      this.ws.on('open', () => {
        clearTimeout(timeout);
        this.connected = true;
        this.emit('connected');
        resolve();
      });

      this.ws.on('message', (data: WebSocket.RawData) => {
        try {
          const message = JSON.parse(data.toString()) as CDPResponse | CDPEvent;
          
          if ('id' in message && message.id !== undefined) {
            const pending = this.pendingRequests.get(message.id);
            if (pending) {
              this.pendingRequests.delete(message.id);
              if (message.error) {
                pending.reject(new Error(message.error.message));
              } else {
                pending.resolve(message.result ?? {});
              }
            }
          } else if ('method' in message) {
            this.emit('event', message);
          }
        } catch {
          // Ignore malformed messages
        }
      });

      this.ws.on('error', (err) => {
        clearTimeout(timeout);
        reject(err);
      });

      this.ws.on('close', () => {
        this.connected = false;
        this.emit('disconnected');
        this.attemptReconnect();
      });
    });
  }

  /**
   * Attempt to reconnect the WebSocket with exponential backoff.
   * Only reconnects if the browser process is still alive and we weren't
   * intentionally closed.
   */
  private async attemptReconnect(): Promise<void> {
    if (this.intentionallyClosed || this.isReconnecting) return;

    // Don't reconnect if the process is dead
    if (!this.process || this.process.exitCode !== null) return;

    this.isReconnecting = true;

    while (this.reconnectAttempts < this.maxReconnectAttempts && !this.intentionallyClosed) {
      this.reconnectAttempts++;
      const delay = Math.min(
        this.reconnectBaseDelay * Math.pow(2, this.reconnectAttempts - 1),
        15000 // max 15s
      );

      console.log(`[Reconnect] Instance ${this.id}: attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts} in ${delay}ms`);

      await this.sleep(delay);

      // Check again after sleep — process might have died or we were closed
      if (this.intentionallyClosed || !this.process || this.process.exitCode !== null) break;

      try {
        await this.connectWebSocket();
        this.reconnectAttempts = 0;
        this.isReconnecting = false;
        this.emit('reconnected');
        console.log(`[Reconnect] Instance ${this.id}: reconnected`);
        return;
      } catch {
        // Connection failed, retry
      }
    }

    this.isReconnecting = false;
    this.emit('reconnect_failed');
    console.log(`[Reconnect] Instance ${this.id}: all attempts exhausted`);
  }

  /**
   * Wait for the DOM to settle after a user interaction (click, submit, etc.)
   * Polls document.readyState and DOM size until stable, with a minimum wait.
   */
  private async waitForDomSettle(minWaitMs = 100, maxWaitMs = 3000, pollIntervalMs = 100): Promise<void> {
    await this.sleep(minWaitMs);

    const deadline = Date.now() + maxWaitMs;
    let lastNodeCount = -1;
    let stableCount = 0;

    while (Date.now() < deadline) {
      const check = await this.sendCommand(
        'Runtime.evaluate',
        { expression: 'document.readyState + "|" + document.querySelectorAll("*").length', returnByValue: true }
      ) as { result?: { value?: string } };

      const parts = (check.result?.value ?? '').split('|');
      const readyState = parts[0];
      const nodeCount = parseInt(parts[1] ?? '0', 10);

      if (readyState === 'complete' && nodeCount === lastNodeCount) {
        stableCount++;
        if (stableCount >= 2) return; // DOM stable for 2 consecutive polls
      } else {
        stableCount = 0;
      }

      lastNodeCount = nodeCount;
      await this.sleep(pollIntervalMs);
    }
  }

  private sendCommand(method: string, params?: Record<string, unknown>, timeout?: number): Promise<unknown> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        reject(new Error('WebSocket not connected'));
        return;
      }

      const id = ++this.messageId;
      const message = { id, method, params: params ?? {} };
      
      const timeoutMs = timeout ?? this.requestTimeout;
      const timeoutId = setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error(`Command ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      this.pendingRequests.set(id, {
        resolve: (result) => {
          clearTimeout(timeoutId);
          resolve(result);
        },
        reject: (err) => {
          clearTimeout(timeoutId);
          reject(err);
        },
      });

      this.ws.send(JSON.stringify(message));
    });
  }

  // Navigation with optional custom headers
  async navigate(url: string, options?: { 
    waitMs?: number; 
    interactiveOnly?: boolean;
    headers?: Record<string, string>;
  }): Promise<BrowserNavigateResult> {
    try {
      const result = await this.sendCommand(
        'Page.navigate',
        { 
          url,
          waitMs: options?.waitMs ?? 3000,
          interactiveOnly: options?.interactiveOnly ?? false,
          headers: options?.headers,
        },
        this.navigateTimeout
      ) as { 
        frameId: string; 
        title?: string;
        semanticTree?: {
          markdown: string;
          stats: BrowserNavigateResult['stats'];
        };
      };

      this.currentUrl = url;

      let markdown = result.semanticTree?.markdown ?? '';
      let stats = result.semanticTree?.stats ?? {
        landmarks: 0, links: 0, headings: 0, actions: 0, forms: 0, totalNodes: 0
      };

      if (!markdown) {
        const treeResult = await this.sendCommand(
          'Runtime.evaluate',
          { expression: 'document.semanticTree || document.body.innerText' }
        ) as { result?: { value?: string } };
        markdown = treeResult.result?.value ?? '';
      }

      return {
        success: true,
        url,
        title: result.title,
        markdown,
        stats,
      };
    } catch (error) {
      return {
        success: false,
        url,
        markdown: '',
        stats: { landmarks: 0, links: 0, headings: 0, actions: 0, forms: 0, totalNodes: 0 },
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async click(elementId: string): Promise<BrowserClickResult> {
    try {
      const result = await this.sendCommand(
        'Runtime.evaluate',
        { 
          expression: `
            (function() {
              const el = document.querySelector('[data-pardus-id="${elementId}"]');
              if (!el) return { success: false, error: 'Element not found' };
              el.click();
              return { success: true, navigated: false };
            })()
          `,
          returnByValue: true
        }
      ) as { result?: { value?: { success: boolean; navigated: boolean; error?: string } } };

      const value = result.result?.value;
      if (!value?.success) {
        return {
          success: false,
          navigated: false,
          error: value?.error || 'Click failed',
        };
      }

      await this.waitForDomSettle();

      const pageInfo = await this.sendCommand('Page.getNavigationHistory', {}) as {
        currentIndex: number;
        entries: Array<{ url: string; title: string }>;
      };
      
      const currentEntry = pageInfo.entries[pageInfo.currentIndex];
      const navigated = currentEntry.url !== this.currentUrl;
      this.currentUrl = currentEntry.url;

      const treeResult = await this.sendCommand(
        'Runtime.evaluate',
        { expression: 'document.semanticTree || document.body.innerText' }
      ) as { result?: { value?: string } };

      return {
        success: true,
        navigated,
        url: currentEntry.url,
        markdown: treeResult.result?.value ?? '',
        stats: { landmarks: 0, links: 0, headings: 0, actions: 0, forms: 0, totalNodes: 0 },
      };
    } catch (error) {
      return {
        success: false,
        navigated: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async fill(elementId: string, value: string): Promise<BrowserFillResult> {
    try {
      const result = await this.sendCommand(
        'Runtime.evaluate',
        { 
          expression: `
            (function() {
              const el = document.querySelector('[data-pardus-id="${elementId}"]');
              if (!el) return { success: false, error: 'Element not found' };
              if (el.tagName !== 'INPUT' && el.tagName !== 'TEXTAREA') {
                return { success: false, error: 'Element is not an input' };
              }
              el.value = ${JSON.stringify(value)};
              el.dispatchEvent(new Event('input', { bubbles: true }));
              el.dispatchEvent(new Event('change', { bubbles: true }));
              return { success: true };
            })()
          `,
          returnByValue: true
        }
      ) as { result?: { value?: { success: boolean; error?: string } } };

      const res = result.result?.value;
      return {
        success: res?.success ?? false,
        error: res?.error,
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async submit(formElementId?: string): Promise<BrowserSubmitResult> {
    try {
      const result = await this.sendCommand(
        'Runtime.evaluate',
        { 
          expression: formElementId ? `
            (function() {
              const form = document.querySelector('[data-pardus-id="${formElementId}"]');
              if (!form) return { success: false, error: 'Form not found' };
              form.submit();
              return { success: true, navigated: true };
            })()
          ` : `
            (function() {
              const form = document.querySelector('form');
              if (!form) return { success: false, error: 'No form found' };
              form.submit();
              return { success: true, navigated: true };
            })()
          `,
          returnByValue: true
        }
      ) as { result?: { value?: { success: boolean; navigated: boolean; error?: string } } };

      const value = result.result?.value;
      if (!value?.success) {
        return {
          success: false,
          navigated: false,
          error: value?.error || 'Submit failed',
        };
      }

      await this.waitForDomSettle(200, 5000);

      const pageInfo = await this.sendCommand('Page.getNavigationHistory', {}) as {
        currentIndex: number;
        entries: Array<{ url: string; title: string }>;
      };
      
      const currentEntry = pageInfo.entries[pageInfo.currentIndex];
      this.currentUrl = currentEntry.url;

      const treeResult = await this.sendCommand(
        'Runtime.evaluate',
        { expression: 'document.semanticTree || document.body.innerText' }
      ) as { result?: { value?: string } };

      return {
        success: true,
        navigated: true,
        url: currentEntry.url,
        markdown: treeResult.result?.value ?? '',
        stats: { landmarks: 0, links: 0, headings: 0, actions: 0, forms: 0, totalNodes: 0 },
      };
    } catch (error) {
      return {
        success: false,
        navigated: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async scroll(direction: 'up' | 'down' | 'top' | 'bottom'): Promise<BrowserScrollResult> {
    try {
      const scrollScript = {
        up: 'window.scrollBy(0, -window.innerHeight * 0.8)',
        down: 'window.scrollBy(0, window.innerHeight * 0.8)',
        top: 'window.scrollTo(0, 0)',
        bottom: 'window.scrollTo(0, document.body.scrollHeight)',
      }[direction];

      await this.sendCommand('Runtime.evaluate', { expression: scrollScript });

      // Wait briefly for any lazy-loaded content to start loading
      await this.sleep(300);

      // Fetch the updated semantic tree
      const treeResult = await this.sendCommand(
        'Runtime.evaluate',
        { expression: 'document.semanticTree || document.body.innerText' }
      ) as { result?: { value?: string } };

      return {
        success: true,
        markdown: treeResult.result?.value ?? '',
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // Cookie Management
  async getCookies(url?: string): Promise<BrowserGetCookiesResult> {
    try {
      const targetUrl = url || this.currentUrl;
      if (!targetUrl) {
        return { success: false, cookies: [], error: 'No URL specified' };
      }

      const result = await this.sendCommand(
        'Network.getCookies',
        { urls: [targetUrl] }
      ) as { cookies: Array<{
        name: string;
        value: string;
        domain: string;
        path: string;
        expires: number;
        httpOnly: boolean;
        secure: boolean;
        sameSite: 'Strict' | 'Lax' | 'None';
      }> };

      const cookies: Cookie[] = result.cookies.map(c => ({
        name: c.name,
        value: c.value,
        domain: c.domain,
        path: c.path,
        expires: c.expires,
        httpOnly: c.httpOnly,
        secure: c.secure,
        sameSite: c.sameSite,
      }));

      return { success: true, cookies };
    } catch (error) {
      return {
        success: false,
        cookies: [],
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async setCookie(
    name: string,
    value: string,
    options?: {
      url?: string;
      domain?: string;
      path?: string;
      expires?: number;
      httpOnly?: boolean;
      secure?: boolean;
      sameSite?: 'Strict' | 'Lax' | 'None';
    }
  ): Promise<BrowserSetCookieResult> {
    try {
      const url = options?.url || this.currentUrl;
      if (!url) {
        return { success: false, error: 'No URL specified' };
      }

      await this.sendCommand('Network.setCookie', {
        name,
        value,
        url,
        domain: options?.domain,
        path: options?.path || '/',
        expires: options?.expires,
        httpOnly: options?.httpOnly,
        secure: options?.secure,
        sameSite: options?.sameSite,
      });

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async deleteCookie(name: string, url?: string): Promise<BrowserDeleteCookieResult> {
    try {
      const targetUrl = url || this.currentUrl;
      if (!targetUrl) {
        return { success: false, error: 'No URL specified' };
      }

      await this.sendCommand('Network.deleteCookies', {
        name,
        url: targetUrl,
      });

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // Storage Management
  async getStorage(
    storageType: 'localStorage' | 'sessionStorage',
    key?: string
  ): Promise<BrowserGetStorageResult> {
    try {
      const expression = key
        ? `JSON.stringify({ "${key}": ${storageType}.getItem("${key}") })`
        : `JSON.stringify(Object.fromEntries(Object.entries(${storageType})))`;

      const result = await this.sendCommand('Runtime.evaluate', {
        expression,
        returnByValue: true,
      }) as { result?: { value?: string } };

      const items: StorageItem[] = [];
      if (result.result?.value) {
        const parsed = JSON.parse(result.result.value);
        for (const [k, v] of Object.entries(parsed)) {
          items.push({ key: k, value: v as string });
        }
      }

      return { success: true, items };
    } catch (error) {
      return {
        success: false,
        items: [],
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async setStorage(
    storageType: 'localStorage' | 'sessionStorage',
    key: string,
    value: string
  ): Promise<BrowserSetStorageResult> {
    try {
      await this.sendCommand('Runtime.evaluate', {
        expression: `${storageType}.setItem("${key}", ${JSON.stringify(value)})`,
      });

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async deleteStorage(
    storageType: 'localStorage' | 'sessionStorage',
    key: string
  ): Promise<BrowserDeleteStorageResult> {
    try {
      await this.sendCommand('Runtime.evaluate', {
        expression: `${storageType}.removeItem("${key}")`,
      });

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async clearStorage(
    storageType: 'localStorage' | 'sessionStorage' | 'both'
  ): Promise<BrowserClearStorageResult> {
    try {
      if (storageType === 'localStorage' || storageType === 'both') {
        await this.sendCommand('Runtime.evaluate', {
          expression: 'localStorage.clear()',
        });
      }
      if (storageType === 'sessionStorage' || storageType === 'both') {
        await this.sendCommand('Runtime.evaluate', {
          expression: 'sessionStorage.clear()',
        });
      }

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async getActionPlan(): Promise<BrowserGetActionPlanResult> {
    try {
      const result = await this.sendCommand('Pardus.getActionPlan', {}) as {
        actionPlan?: BrowserGetActionPlanResult['actionPlan'];
      };

      return {
        success: true,
        actionPlan: result.actionPlan,
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async autoFill(fields: Array<{ key: string; value: string }>): Promise<BrowserAutoFillResult> {
    try {
      const fieldsMap: Record<string, string> = {};
      for (const { key, value } of fields) {
        fieldsMap[key] = value;
      }

      const result = await this.sendCommand('Pardus.autoFill', {
        fields: fieldsMap,
      }) as {
        filled_fields?: BrowserAutoFillResult['filledFields'];
        unmatched_fields?: BrowserAutoFillResult['unmatchedFields'];
      };

      return {
        success: true,
        filledFields: result.filled_fields,
        unmatchedFields: result.unmatched_fields,
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async getCurrentState(): Promise<{ url: string; title?: string; markdown: string }> {
    try {
      const [history, treeResult] = await Promise.all([
        this.sendCommand('Page.getNavigationHistory', {}) as Promise<{
          currentIndex: number;
          entries: Array<{ url: string; title: string }>;
        }>,
        this.sendCommand('Runtime.evaluate', { 
          expression: 'document.semanticTree || document.body.innerText' 
        }) as Promise<{ result?: { value?: string } }>,
      ]);

      const currentEntry = history.entries[history.currentIndex];
      
      return {
        url: currentEntry.url,
        title: currentEntry.title,
        markdown: treeResult.result?.value ?? '',
      };
    } catch (error) {
      return {
        url: this.currentUrl ?? '',
        title: '',
        markdown: '',
      };
    }
  }

  async wait(
    condition: 'contentLoaded' | 'contentStable' | 'networkIdle' | 'minInteractive' | 'selector',
    options?: { selector?: string; minCount?: number; timeoutMs?: number; intervalMs?: number }
  ): Promise<BrowserWaitResult> {
    try {
      const result = await this.sendCommand('Pardus.wait', {
        condition,
        selector: options?.selector,
        minCount: options?.minCount,
        timeoutMs: options?.timeoutMs ?? 10000,
        intervalMs: options?.intervalMs ?? 500,
      }, options?.timeoutMs ?? 10000) as {
        satisfied: boolean;
        condition: string;
        reason?: string;
      };

      return {
        success: true,
        satisfied: result.satisfied,
        condition: result.condition,
        reason: result.reason,
      };
    } catch (error) {
      return {
        success: false,
        satisfied: false,
        condition,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Extraction methods ────────────────────────────────────────

  async extractText(selector?: string): Promise<BrowserExtractTextResult> {
    try {
      const scopeExpr = selector
        ? `document.querySelector("${selector.replace(/"/g, '\\"')}")`
        : 'document.body';

      const result = await this.sendCommand(
        'Runtime.evaluate',
        {
          expression: `(function() {
            const root = ${scopeExpr};
            if (!root) return { error: "Element not found: ${selector?.replace(/"/g, '\\"')}" };
            const clone = root.cloneNode(true);
            const remove = ['script','style','noscript','svg','iframe','nav','footer','header','aside','[role="navigation"]','[role="banner"]','[role="contentinfo"]','[role="complementary"]','.ad','.ads','.advertisement','.sidebar','.cookie-banner','.popup','.modal'];
            for (const sel of remove) {
              clone.querySelectorAll(sel).forEach(el => el.remove());
            }
            clone.querySelectorAll('*').forEach(el => {
              const style = el.getAttribute('style') || '';
              if (style.includes('display:none') || style.includes('display: none') || style.includes('visibility:hidden') || style.includes('visibility: hidden')) {
                el.remove();
              }
            });
            let text = clone.textContent || '';
            text = text.replace(/\\s+/g, ' ').replace(/\\s+\\n/g, '\\n').trim();
            const wordCount = text.split(/\\s+/).filter(w => w.length > 0).length;
            return { text, wordCount };
          })()`,
          returnByValue: true,
        }
      ) as { result?: { value?: { text?: string; wordCount?: number; error?: string } } };

      const value = result.result?.value;
      if (value?.error) {
        return { success: false, text: '', word_count: 0, error: value.error };
      }

      return {
        success: true,
        text: value?.text ?? '',
        word_count: value?.wordCount ?? 0,
      };
    } catch (error) {
      return {
        success: false,
        text: '',
        word_count: 0,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async extractLinks(filter?: string, domain?: string): Promise<BrowserExtractLinksResult> {
    try {
      const result = await this.sendCommand(
        'Runtime.evaluate',
        {
          expression: `(function() {
            const links = Array.from(document.querySelectorAll('a[href]'));
            const filterLower = ${JSON.stringify(filter?.toLowerCase() ?? '')};
            const domainFilter = ${JSON.stringify(domain?.toLowerCase() ?? '')};
            const mapped = links.map(a => ({
              text: (a.textContent || '').trim().substring(0, 200),
              href: a.href,
              element_id: a.getAttribute('data-pardus-id') || null,
            })).filter(l => {
              if (!l.href || l.href === '#' || l.href.startsWith('javascript:')) return false;
              if (filterLower && !l.text.toLowerCase().includes(filterLower) && !l.href.toLowerCase().includes(filterLower)) return false;
              if (domainFilter) {
                try { if (!new URL(l.href).hostname.toLowerCase().includes(domainFilter)) return false; } catch { return false; }
              }
              return true;
            });
            return { links: mapped, count: mapped.length };
          })()`,
          returnByValue: true,
        }
      ) as { result?: { value?: { links?: Array<{ text: string; href: string; element_id?: string | null }>; count?: number } } };

      const value = result.result?.value;
      const links: LinkItem[] = (value?.links ?? []).map(l => ({
        text: l.text,
        href: l.href,
        ...(l.element_id ? { element_id: l.element_id } : {}),
      }));

      return {
        success: true,
        links,
        count: value?.count ?? links.length,
      };
    } catch (error) {
      return {
        success: false,
        links: [],
        count: 0,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async find(query: string, caseSensitive = false): Promise<BrowserFindResult> {
    try {
      const result = await this.sendCommand(
        'Runtime.evaluate',
        {
          expression: `(function() {
            const query = ${JSON.stringify(query)};
            const caseSensitive = ${caseSensitive};
            const matches = [];
            const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null);
            while (walker.nextNode()) {
              const node = walker.currentNode;
              const text = node.textContent;
              if (!text) continue;
              const searchIn = caseSensitive ? text : text.toLowerCase();
              const searchFor = caseSensitive ? query : query.toLowerCase();
              let idx = searchIn.indexOf(searchFor);
              while (idx !== -1) {
                const start = Math.max(0, idx - 50);
                const end = Math.min(text.length, idx + query.length + 50);
                const context = (start > 0 ? '...' : '') + text.substring(start, end) + (end < text.length ? '...' : '');
                const parent = node.parentElement;
                matches.push({
                  text: text.substring(idx, idx + query.length),
                  context,
                  element_id: parent ? parent.getAttribute('data-pardus-id') || null : null,
                });
                if (matches.length >= 50) return { matches, count: matches.length, truncated: true };
                idx = searchIn.indexOf(searchFor, idx + 1);
              }
            }
            return { matches, count: matches.length, truncated: false };
          })()`,
          returnByValue: true,
        }
      ) as { result?: { value?: { matches?: TextMatch[]; count?: number; truncated?: boolean } } };

      const value = result.result?.value;
      return {
        success: true,
        matches: value?.matches ?? [],
        count: value?.count ?? 0,
      };
    } catch (error) {
      return {
        success: false,
        matches: [],
        count: 0,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async extractTable(selector?: string): Promise<BrowserExtractTableResult> {
    try {
      const tableSelector = selector ? `"${selector.replace(/"/g, '\\"')}"` : '"table"';
      const result = await this.sendCommand(
        'Runtime.evaluate',
        {
          expression: `(function() {
            const table = document.querySelector(${tableSelector});
            if (!table) return { error: "Table not found" };
            const headers = [];
            const rows = [];
            const thCells = table.querySelectorAll('thead th, tr:first-child th');
            thCells.forEach(th => headers.push(th.textContent.trim()));
            const bodyRows = table.querySelectorAll('tbody tr, tr');
            bodyRows.forEach((tr, idx) => {
              if (idx === 0 && thCells.length > 0 && tr.querySelector('th')) return;
              const cells = Array.from(tr.querySelectorAll('td, th')).map(c => c.textContent.trim());
              if (cells.length > 0) rows.push(cells);
            });
            if (headers.length === 0 && rows.length > 0) {
              rows[0].forEach(() => headers.push(''));
            }
            return { headers, rows, row_count: rows.length };
          })()`,
          returnByValue: true,
        }
      ) as { result?: { value?: { headers?: string[]; rows?: string[][]; row_count?: number; error?: string } } };

      const value = result.result?.value;
      if (value?.error) {
        return { success: false, headers: [], rows: [], row_count: 0, error: value.error };
      }

      return {
        success: true,
        headers: value?.headers ?? [],
        rows: value?.rows ?? [],
        row_count: value?.row_count ?? 0,
      };
    } catch (error) {
      return {
        success: false,
        headers: [],
        rows: [],
        row_count: 0,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async extractMetadata(): Promise<BrowserExtractMetadataResult> {
    try {
      const result = await this.sendCommand(
        'Runtime.evaluate',
        {
          expression: `(function() {
            const title = document.title || '';
            const descMeta = document.querySelector('meta[name="description"]');
            const description = descMeta ? descMeta.getAttribute('content') || '' : '';
            const jsonLd = Array.from(document.querySelectorAll('script[type="application/ld+json"]'))
              .map(s => { try { return JSON.parse(s.textContent); } catch { return null; } })
              .filter(v => v !== null);
            const og = {};
            document.querySelectorAll('meta[property^="og:"]').forEach(m => {
              const prop = m.getAttribute('property');
              const content = m.getAttribute('content');
              if (prop && content) og[prop.replace('og:', '')] = content;
            });
            const meta = {};
            document.querySelectorAll('meta[name]').forEach(m => {
              const name = m.getAttribute('name');
              const content = m.getAttribute('content');
              if (name && content && name !== 'viewport' && name !== 'charset') {
                meta[name] = content;
              }
            });
            return { title, description, json_ld: jsonLd, open_graph: og, meta };
          })()`,
          returnByValue: true,
        }
      ) as { result?: { value?: BrowserExtractMetadataResult } };

      const value = result.result?.value;
      return {
        success: true,
        title: value?.title ?? '',
        description: value?.description,
        json_ld: value?.json_ld ?? [],
        open_graph: value?.open_graph ?? {},
        meta: value?.meta ?? {},
      };
    } catch (error) {
      return {
        success: false,
        title: '',
        json_ld: [],
        open_graph: {},
        meta: {},
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async screenshot(): Promise<BrowserScreenshotResult> {
    try {
      const result = await this.sendCommand(
        'Page.captureScreenshot',
        { format: 'png' },
        this.requestTimeout
      ) as { data?: string; mimeType?: string };

      return {
        success: true,
        data: result.data ?? '',
        mime_type: result.mimeType ?? 'image/png',
      };
    } catch (error) {
      return {
        success: false,
        data: '',
        mime_type: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Interaction methods ───────────────────────────────────────

  async selectOption(elementId: string, value: string): Promise<BrowserSelectResult> {
    try {
      const result = await this.sendCommand(
        'Runtime.evaluate',
        {
          expression: `(function() {
            const el = document.querySelector('[data-pardus-id="${elementId}"]');
            if (!el) return { success: false, error: 'Element not found' };
            if (el.tagName !== 'SELECT') return { success: false, error: 'Element is not a <select> dropdown' };
            el.value = ${JSON.stringify(value)};
            el.dispatchEvent(new Event('change', { bubbles: true }));
            return { success: true, selected_value: el.value };
          })()`,
          returnByValue: true,
        }
      ) as { result?: { value?: { success: boolean; selected_value?: string; error?: string } } };

      const val = result.result?.value;
      return {
        success: val?.success ?? false,
        selected_value: val?.selected_value ?? value,
        error: val?.error,
      };
    } catch (error) {
      return {
        success: false,
        selected_value: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async pressKey(key: string): Promise<BrowserPressKeyResult> {
    try {
      const keyMap: Record<string, { key: string; code: string; keyCode: number; text?: string }> = {
        'Enter':      { key: 'Enter',      code: 'Enter',      keyCode: 13 },
        'Tab':        { key: 'Tab',         code: 'Tab',        keyCode: 9 },
        'Escape':     { key: 'Escape',      code: 'Escape',     keyCode: 27 },
        'Backspace':  { key: 'Backspace',   code: 'Backspace',  keyCode: 8 },
        'Delete':     { key: 'Delete',      code: 'Delete',     keyCode: 46 },
        'ArrowUp':    { key: 'ArrowUp',     code: 'ArrowUp',    keyCode: 38 },
        'ArrowDown':  { key: 'ArrowDown',   code: 'ArrowDown',  keyCode: 40 },
        'ArrowLeft':  { key: 'ArrowLeft',   code: 'ArrowLeft',  keyCode: 37 },
        'ArrowRight': { key: 'ArrowRight',  code: 'ArrowRight', keyCode: 39 },
        'Home':       { key: 'Home',        code: 'Home',       keyCode: 36 },
        'End':        { key: 'End',         code: 'End',        keyCode: 35 },
        'PageUp':     { key: 'PageUp',      code: 'PageUp',     keyCode: 33 },
        'PageDown':   { key: 'PageDown',    code: 'PageDown',   keyCode: 34 },
        ' ':          { key: ' ',           code: 'Space',      keyCode: 32, text: ' ' },
      };

      const keyInfo = keyMap[key] ?? { key, code: `Key${key.toUpperCase()}`, keyCode: key.charCodeAt(0), text: key };

      // Dispatch keyDown, char (if printable), keyUp
      const events = ['keyDown', 'keyUp'];
      if (keyInfo.text) events.splice(1, 0, 'char');

      for (const type of events) {
        await this.sendCommand('Input.dispatchKeyEvent', {
          type,
          key: keyInfo.key,
          code: keyInfo.code,
          windowsVirtualKeyCode: keyInfo.keyCode,
          ...(type === 'char' && keyInfo.text ? { text: keyInfo.text } : {}),
        });
      }

      await this.waitForDomSettle(50, 1000);

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async hover(elementId: string): Promise<BrowserHoverResult> {
    try {
      const result = await this.sendCommand(
        'Runtime.evaluate',
        {
          expression: `(function() {
            const el = document.querySelector('[data-pardus-id="${elementId}"]');
            if (!el) return { success: false, error: 'Element not found' };
            const events = ['pointerenter','pointerover','mouseover','mouseenter'];
            for (const type of events) {
              el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window }));
            }
            return { success: true };
          })()`,
          returnByValue: true,
        }
      ) as { result?: { value?: { success: boolean; error?: string } } };

      const val = result.result?.value;
      if (!val?.success) {
        return { success: false, error: val?.error || 'Hover failed' };
      }

      await this.waitForDomSettle(100, 2000);

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Tab management methods ────────────────────────────────────

  async newTab(url: string): Promise<BrowserTabNewResult> {
    try {
      const result = await this.sendCommand(
        'Target.createTarget',
        { url }
      ) as { targetId?: string };

      return {
        success: true,
        target_id: result.targetId ?? '',
      };
    } catch (error) {
      return {
        success: false,
        target_id: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async switchTab(targetId: string): Promise<BrowserTabSwitchResult> {
    try {
      await this.sendCommand('Target.activateTarget', { targetId });

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async closeTab(targetId: string): Promise<BrowserTabCloseResult> {
    try {
      await this.sendCommand('Target.closeTarget', { targetId });

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async listTabs(): Promise<BrowserTabListResult> {
    try {
      const result = await this.sendCommand(
        'Target.getTargets',
        {}
      ) as { targetInfos?: Array<{ targetId: string; url: string; title: string; type: string }> };

      const tabs: TabInfo[] = (result.targetInfos ?? [])
        .filter(t => t.type === 'page')
        .map(t => ({
          target_id: t.targetId,
          url: t.url,
          title: t.title,
          active: false,
        }));

      return {
        success: true,
        tabs,
      };
    } catch (error) {
      return {
        success: false,
        tabs: [],
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Download/Upload methods ─────────────────────────────────────

  async download(url: string, filename?: string): Promise<BrowserDownloadResult> {
    try {
      const result = await this.sendCommand(
        'Pardus.download',
        { url, filename },
        60000
      ) as { path?: string; size_bytes?: number; mime_type?: string };

      return {
        success: true,
        path: result.path ?? '',
        size_bytes: result.size_bytes ?? 0,
        mime_type: result.mime_type,
      };
    } catch (error) {
      return {
        success: false,
        path: '',
        size_bytes: 0,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async upload(elementId: string, filePath: string): Promise<BrowserUploadResult> {
    try {
      await this.sendCommand('DOM.setFileInputFiles', {
        elementId,
        files: [filePath],
      });

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Content methods ─────────────────────────────────────────────

  async pdfExtract(url: string): Promise<BrowserPdfExtractResult> {
    try {
      const result = await this.sendCommand(
        'Pardus.pdfExtract',
        { url },
        60000
      ) as { text?: string; page_count?: number; tables?: string[][]; forms?: Record<string, string>[] };

      return {
        success: true,
        text: result.text ?? '',
        page_count: result.page_count ?? 0,
        tables: result.tables,
        forms: result.forms,
      };
    } catch (error) {
      return {
        success: false,
        text: '',
        page_count: 0,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async feedParse(url: string): Promise<BrowserFeedParseResult> {
    try {
      const result = await this.sendCommand(
        'Pardus.feedParse',
        { url },
        30000
      ) as { feed_type?: string; title?: string; description?: string; items?: FeedItem[] };

      return {
        success: true,
        feed_type: (result.feed_type as 'rss' | 'atom') ?? 'rss',
        title: result.title ?? '',
        description: result.description,
        items: result.items ?? [],
        item_count: result.items?.length ?? 0,
      };
    } catch (error) {
      return {
        success: false,
        feed_type: 'rss',
        title: '',
        items: [],
        item_count: 0,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Network control methods ─────────────────────────────────────

  async networkBlock(resourceTypes: string[]): Promise<BrowserNetworkBlockResult> {
    try {
      await this.sendCommand('Pardus.networkBlock', {
        resource_types: resourceTypes,
      });

      return {
        success: true,
        blocked_types: resourceTypes,
      };
    } catch (error) {
      return {
        success: false,
        blocked_types: [],
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async networkLog(filter?: string): Promise<BrowserNetworkLogResult> {
    try {
      const result = await this.sendCommand(
        'Pardus.networkLog',
        { filter }
      ) as { requests?: Array<{
        url: string;
        method: string;
        status: number;
        mime_type: string;
        size_bytes: number;
        duration_ms: number;
      }> };

      return {
        success: true,
        requests: result.requests ?? [],
        count: result.requests?.length ?? 0,
      };
    } catch (error) {
      return {
        success: false,
        requests: [],
        count: 0,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Iframe methods ──────────────────────────────────────────────

  async iframeEnter(elementId: string): Promise<BrowserIframeEnterResult> {
    try {
      await this.sendCommand('Pardus.iframeEnter', { elementId });

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async iframeExit(): Promise<BrowserIframeExitResult> {
    try {
      await this.sendCommand('Pardus.iframeExit', {});

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  // ── Page diff method ────────────────────────────────────────────

  async diff(): Promise<BrowserDiffResult> {
    try {
      const result = await this.sendCommand(
        'Pardus.diff',
        {}
      ) as { changes?: PageDiffChange[]; change_count?: number; summary?: string };

      return {
        success: true,
        changes: result.changes ?? [],
        change_count: result.change_count ?? 0,
        summary: result.summary ?? '',
      };
    } catch (error) {
      return {
        success: false,
        changes: [],
        change_count: 0,
        summary: '',
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  kill(): void {
    this.intentionallyClosed = true;
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    if (this.process) {
      this.process.kill('SIGTERM');
      setTimeout(() => {
        this.process?.kill('SIGKILL');
      }, 5000);
    }
    this.connected = false;
  }

  isConnected(): boolean {
    return this.connected && this.ws?.readyState === WebSocket.OPEN;
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}
