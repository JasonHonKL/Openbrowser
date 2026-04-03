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
      });
    });
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

      await this.sleep(500);

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

      await this.sleep(1000);

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
      
      return { success: true };
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

  kill(): void {
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
