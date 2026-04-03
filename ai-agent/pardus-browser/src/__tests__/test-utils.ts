/**
 * Test utilities and mocks
 */

import { EventEmitter } from 'node:events';
import type { BrowserManager } from '../core/BrowserManager.js';
import type { BrowserInstance } from '../core/BrowserInstance.js';

// Mock WebSocket for testing
export class MockWebSocket extends EventEmitter {
  readyState = 1; // OPEN
  sentMessages: unknown[] = [];
  
  constructor(public url: string) {
    super();
    // Simulate async connection
    setTimeout(() => this.emit('open'), 10);
  }
  
  send(data: string | Buffer): void {
    this.sentMessages.push(JSON.parse(data.toString()));
  }
  
  close(): void {
    this.readyState = 3; // CLOSED
    this.emit('close');
  }
  
  // Simulate receiving a message from server
  simulateMessage(data: unknown): void {
    this.emit('message', JSON.stringify(data));
  }
  
  // Simulate an error
  simulateError(error: Error): void {
    this.emit('error', error);
  }
}

// Mock child process
export class MockChildProcess extends EventEmitter {
  pid = 12345;
  killed = false;
  
  stdout = new EventEmitter();
  stderr = new EventEmitter();
  
  kill(signal?: string): boolean {
    this.killed = true;
    this.emit('exit', 0, signal);
    return true;
  }
  
  // Simulate browser ready
  simulateReady(): void {
    setTimeout(() => {
      this.stdout.emit('data', 'CDP server ready on port 9222');
    }, 50);
  }
  
  // Simulate error
  simulateError(message: string): void {
    setTimeout(() => {
      this.stderr.emit('data', message);
      this.emit('error', new Error(message));
    }, 50);
  }
}

// Create a mock BrowserManager for testing
export function createMockBrowserManager(): BrowserManager {
  const instances = new Map<string, BrowserInstance>();
  const instanceUrls = new Map<string, string>();
  
  return {
    createInstance: async (options?: { id?: string; proxy?: string }) => {
      const id = options?.id || `browser_${Date.now()}`;
      // Return a minimal mock instance
      const mockInstance = {
        id,
        port: 9222,
        currentUrl: undefined,
        isConnected: () => true,
        navigate: async (url: string, opts?: { waitMs?: number; interactiveOnly?: boolean; headers?: Record<string, string> }) => {
          instanceUrls.set(id, url);
          return {
            success: true,
            url,
            markdown: `# Example\n\n[#1 Link] Test\n\nURL: ${url}`,
            stats: { landmarks: 1, links: 1, headings: 1, actions: 1, forms: 0, totalNodes: 4 },
          };
        },
        click: async () => ({ success: true, navigated: false }),
        fill: async () => ({ success: true }),
        submit: async () => ({ success: true, navigated: true, url: instanceUrls.get(id) || 'https://example.com' }),
        scroll: async () => ({ success: true }),
        getCookies: async () => ({ success: true, cookies: [] }),
        setCookie: async () => ({ success: true }),
        deleteCookie: async () => ({ success: true }),
        getStorage: async () => ({ success: true, items: [] }),
        setStorage: async () => ({ success: true }),
        deleteStorage: async () => ({ success: true }),
        clearStorage: async () => ({ success: true }),
        getCurrentState: async () => ({ 
          url: instanceUrls.get(id) || 'https://example.com', 
          title: 'Example', 
          markdown: '' 
        }),
        getActionPlan: async () => ({
          success: true,
          actionPlan: {
            url: instanceUrls.get(id) || 'https://example.com',
            suggestions: [
              { action_type: 'Click', reason: 'Submit button found', confidence: 0.95, selector: 'button[type="submit"]' },
              { action_type: 'Fill', reason: 'Empty email field', confidence: 0.9, selector: 'input[name="email"]', label: 'Email' },
            ],
            page_type: 'FormPage',
            has_forms: true,
            has_pagination: false,
            interactive_count: 5,
          },
        }),
        autoFill: async () => ({
          success: true,
          filledFields: [
            { field_name: 'email', value: 'test@example.com', matched_by: 'ByName' },
            { field_name: 'password', value: 'secret123', matched_by: 'ByType' },
          ],
          unmatchedFields: [
            { field_type: 'text', label: 'Phone', placeholder: 'Enter phone', required: false, field_name: 'phone' },
          ],
        }),
        wait: async (condition: string) => ({
          success: true,
          satisfied: true,
          condition,
          reason: condition === 'contentLoaded' ? 'content-loaded' : 'content-stable',
        }),
        kill: () => {},
        on: function(event: string, handler: () => void) {
          return this;
        },
        emit: () => true,
      } as unknown as BrowserInstance;
      
      instances.set(id, mockInstance);
      return mockInstance;
    },
    getInstance: (id: string) => instances.get(id),
    hasInstance: (id: string) => instances.has(id),
    closeInstance: async (id: string) => {
      if (!instances.has(id)) {
        throw new Error(`Browser instance "${id}" not found`);
      }
      instances.delete(id);
      instanceUrls.delete(id);
    },
    closeAll: async () => {
      instances.clear();
      instanceUrls.clear();
    },
    listInstances: () => Array.from(instances.values()).map(i => ({
      id: i.id,
      url: instanceUrls.get(i.id),
      connected: i.isConnected(),
      port: i.port,
    })),
    get instanceCount() {
      return instances.size;
    },
  } as unknown as BrowserManager;
}

// Test timeout helper
export function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Async test wrapper with timeout
export async function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number = 5000,
  message: string = 'Test timed out'
): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(message)), timeoutMs)
    ),
  ]);
}
