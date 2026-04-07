import { ChildProcess, spawn } from "child_process";
import { chromium, Browser, BrowserType } from "playwright-core";
import * as net from "net";
import * as http from "http";

export interface PardusLaunchOptions {
  headless?: boolean;
  host?: string;
  port?: number;
  timeout?: number;
  binaryPath?: string;
}

export interface PardusPlaywrightOptions {
  chromium?: PardusLaunchOptions;
  firefox?: PardusLaunchOptions;
  webkit?: PardusLaunchOptions;
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

    this.process.on("error", (err) => {
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
          let data = "";
          res.on("data", (_chunk: Buffer) => { data += _chunk; });
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

class PardusBrowserType {
  private launcher: PardusLauncher;
  private nativeType: BrowserType;

  constructor(nativeType: BrowserType, launcher: PardusLauncher) {
    this.nativeType = nativeType;
    this.launcher = launcher;
  }

  async launch(options: Record<string, unknown> = {}): Promise<Browser> {
    const cdpUrl = await this.launcher.start();
    return this.nativeType.connectOverCDP(cdpUrl, {
      timeout: this.launcher.timeout * 1000,
    });
  }

  async connectOverCDP(endpointURL: string, options?: Record<string, unknown>): Promise<Browser> {
    return this.nativeType.connectOverCDP(endpointURL, options);
  }
}

export interface PardusBrowserContext {
  chromium: PardusBrowserType;
  firefox: BrowserType;
  webkit: BrowserType;
  close(): void;
}

export async function pardusPlaywright(
  options: PardusPlaywrightOptions = {}
): Promise<PardusBrowserContext> {
  const chromiumLauncher = new PardusLauncher(options.chromium ?? {});

  const context: PardusBrowserContext = {
    chromium: new PardusBrowserType(chromium, chromiumLauncher),
    firefox: chromium.launch as unknown as BrowserType,
    webkit: chromium.launch as unknown as BrowserType,
    close: () => {
      chromiumLauncher.stop();
    },
  };

  return context;
}

export { PardusLauncher };
export default pardusPlaywright;
