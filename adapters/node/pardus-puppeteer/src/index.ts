import { ChildProcess, spawn } from "child_process";
import { connect, Browser, BrowserConnectOptions } from "puppeteer-core";
import * as net from "net";
import * as http from "http";

export interface PardusLaunchOptions {
  headless?: boolean;
  host?: string;
  port?: number;
  timeout?: number;
  binaryPath?: string;
}

export interface PardusExtension {
  semanticTree(): Promise<Record<string, unknown>>;
  navigationGraph(): Promise<Record<string, unknown>>;
  detectActions(): Promise<Array<Record<string, unknown>>>;
  clickById(elementId: number): Promise<Record<string, unknown>>;
  typeById(elementId: number, value: string): Promise<Record<string, unknown>>;
  submitForm(formSelector: string, fields: Record<string, string>): Promise<Record<string, unknown>>;
}

class PardusLauncher {
  private process: ChildProcess | null = null;
  private _cdpUrl: string | null = null;
  private host: string;
  private port: number;
  private timeout: number;
  private binaryPath: string;
  private killTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(options: PardusLaunchOptions = {}) {
    this.host = options.host ?? "127.0.0.1";
    this.port = options.port ?? 0;
    this.timeout = options.timeout ?? 10;
    this.binaryPath = options.binaryPath ?? this.findBinary();
  }

  private findBinary(): string {
    if (process.env.PARDUS_BROWSER_PATH) {
      return process.env.PARDUS_BROWSER_PATH;
    }
    return "pardus-browser";
  }

  private getFreePort(): Promise<number> {
    return new Promise((resolve, reject) => {
      const server = net.createServer();
      server.listen(0, "127.0.0.1", () => {
        const addr = server.address();
        if (typeof addr === "object" && addr) {
          const port = (addr as net.AddressInfo).port;
          server.close(() => resolve(port));
        } else {
          reject(new Error("Could not get free port"));
        }
      });
      server.on("error", reject);
    });
  }

  async start(): Promise<string> {
    if (this.port === 0) {
      this.port = await this.getFreePort();
    }

    this.process = spawn(this.binaryPath, [
      "serve",
      "--host", this.host,
      "--port", String(this.port),
    ], {
      stdio: ["pipe", "pipe", "pipe"],
    });

    this.process.stdout?.on("data", () => {});
    this.process.stderr?.on("data", () => {});

    this.process.on("error", (_err) => {
      if (!this.process?.killed) {
        this.stop();
      }
    });

    this._cdpUrl = `http://${this.host}:${this.port}`;

    await this.waitForReady();

    return this._cdpUrl;
  }

  private waitForReady(): Promise<void> {
    return new Promise((resolve, reject) => {
      const deadline = Date.now() + this.timeout * 1000;

      const check = () => {
        if (this.process?.killed || this.process?.exitCode !== null) {
          reject(new Error("pardus-browser exited early"));
          return;
        }

        http.get(`${this._cdpUrl}/json/version`, (res) => {
          res.on("data", () => {});
          res.on("end", () => {
            if (res.statusCode === 200) {
              resolve();
            } else if (Date.now() < deadline) {
              setTimeout(check, 200);
            } else {
              this.stop();
              reject(new Error(`pardus-browser did not start within ${this.timeout}s`));
            }
          });
        }).on("error", () => {
          if (Date.now() < deadline) {
            setTimeout(check, 200);
          } else {
            this.stop();
            reject(new Error(`pardus-browser did not start within ${this.timeout}s`));
          }
        });
      };

      check();
    });
  }

  stop(): void {
    if (this.killTimer) {
      clearTimeout(this.killTimer);
      this.killTimer = null;
    }

    if (this.process && !this.process.killed) {
      const pid = this.process.pid;
      this.process.kill("SIGTERM");
      this.killTimer = setTimeout(() => {
        this.killTimer = null;
        try {
          process.kill(pid!, "SIGKILL");
        } catch {
          // already dead
        }
      }, 5000);
    }
    this.process = null;
    this._cdpUrl = null;
  }

  get cdpUrl(): string | null {
    return this._cdpUrl;
  }
}

interface EnhancedBrowser extends Browser {
  _pardusLauncher?: PardusLauncher;
}

export class PardusPuppeteer {
  private launcher: PardusLauncher;

  constructor(options: PardusLaunchOptions = {}) {
    this.launcher = new PardusLauncher(options);
  }

  async launch(options: PardusLaunchOptions = {}): Promise<EnhancedBrowser> {
    const cdpUrl = await this.launcher.start();
    const browser = (await connect({
      browserURL: cdpUrl,
      ...options,
    } as BrowserConnectOptions)) as EnhancedBrowser;
    browser._pardusLauncher = this.launcher;
    return browser;
  }

  connect(options: { browserURL: string; [key: string]: unknown }): Promise<EnhancedBrowser> {
    return connect(options as BrowserConnectOptions) as Promise<EnhancedBrowser>;
  }

  close(): void {
    this.launcher.stop();
  }
}

export { PardusLauncher };
export default PardusPuppeteer;
