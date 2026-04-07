use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

mod commands;
mod config;

fn build_proxy_config(
    proxy: Option<String>,
    proxy_http: Option<String>,
    proxy_https: Option<String>,
    no_proxy: Option<String>,
    no_proxy_env: bool,
) -> pardus_core::ProxyConfig {
    let mut proxy_config = pardus_core::ProxyConfig::new();
    if let Some(all_proxy) = proxy {
        proxy_config = proxy_config.with_all_proxy(all_proxy);
    }
    if let Some(http) = proxy_http {
        proxy_config = proxy_config.with_http_proxy(http);
    }
    if let Some(https) = proxy_https {
        proxy_config = proxy_config.with_https_proxy(https);
    }
    if let Some(no) = no_proxy {
        proxy_config = proxy_config.with_no_proxy(no);
    }
    if !no_proxy_env {
        proxy_config = proxy_config.merge_env();
    }
    proxy_config
}

fn apply_cert_pinning(
    browser_config: &mut pardus_core::BrowserConfig,
    cert_pin: Vec<String>,
    cert_pin_file: Option<PathBuf>,
    pin_policy: Option<config::PinPolicyArg>,
) {
    if cert_pin.is_empty() && cert_pin_file.is_none() {
        return;
    }
    let mut all_pins = cert_pin;
    if let Some(path) = &cert_pin_file {
        match config::load_pins_from_file(path) {
            Ok(file_pins) => all_pins.extend(file_pins),
            Err(e) => {
                eprintln!("Warning: failed to load cert pin file: {}", e);
            }
        }
    }
    match config::build_cert_pinning_config(&all_pins, pin_policy, true) {
        Ok(pin_config) => {
            browser_config.cert_pinning = Some(pin_config);
        }
        Err(e) => {
            eprintln!("Warning: invalid certificate pin config: {}", e);
        }
    }
}

#[derive(Parser)]
#[command(name = "pardus-browser")]
#[command(
    version,
    about = "Headless browser for AI agents — semantic tree, no pixels"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Navigate to URL, build semantic tree, print it
    Navigate {
        /// URL to navigate to
        url: String,

        /// Output format
        #[arg(short, long, default_value = "md")]
        format: OutputFormatArg,

        /// Only show interactive elements
        #[arg(long)]
        interactive_only: bool,

        /// Wait time in milliseconds for JS execution
        #[arg(long, default_value = "3000")]
        wait_ms: u32,

        /// Enable JavaScript execution (for SPAs)
        #[arg(long)]
        js: bool,

        /// Include navigation graph
        #[arg(long)]
        with_nav: bool,

        /// Use persistent session (save cookies/storage)
        #[arg(long)]
        persistent: bool,

        /// Custom HTTP header (format: "Name: Value")
        #[arg(long)]
        header: Option<String>,

        /// Verbose logging
        #[arg(short, long)]
        verbose: bool,

        /// Capture and display network request table
        #[arg(long)]
        network_log: bool,

        /// Certificate pin (format: "sha256:HASH" or "host=DOMAIN:sha256:HASH")
        #[arg(long = "cert-pin")]
        cert_pin: Vec<String>,

        /// Load certificate pins from file (one per line, # comments)
        #[arg(long = "cert-pin-file")]
        cert_pin_file: Option<PathBuf>,

        /// Certificate pin match policy: require any or all pins to match
        #[arg(long = "pin-policy", value_enum)]
        pin_policy: Option<config::PinPolicyArg>,

        /// Proxy URL for all traffic (HTTP, HTTPS, or SOCKS5)
        #[arg(long)]
        proxy: Option<String>,

        /// Proxy URL for HTTP traffic (overrides --proxy for HTTP)
        #[arg(long)]
        proxy_http: Option<String>,

        /// Proxy URL for HTTPS traffic (overrides --proxy for HTTPS)
        #[arg(long)]
        proxy_https: Option<String>,

        /// Comma-separated list of hosts to bypass proxy (e.g., "localhost,127.0.0.1")
        #[arg(long)]
        no_proxy: Option<String>,

        /// Disable automatic loading of proxy settings from environment variables
        #[arg(long)]
        no_proxy_env: bool,

        /// Export network requests as HAR 1.2 JSON to the given file path
        #[arg(long)]
        har: Option<PathBuf>,

        /// Generate CSS/JS coverage report and write JSON to the given file path
        #[arg(long)]
        coverage: Option<PathBuf>,
    },

    /// Interact with a page (click, type, submit, wait, scroll)
    Interact {
        /// URL to navigate to
        url: String,

        /// Action to perform
        #[command(subcommand)]
        action: InteractAction,

        /// Output format for result page
        #[arg(short, long, default_value = "md")]
        format: OutputFormatArg,

        /// Enable JavaScript execution
        #[arg(long)]
        js: bool,

        /// Wait time for JS execution (ms)
        #[arg(long, default_value = "3000")]
        wait_ms: u32,

        /// Proxy URL for all traffic (HTTP, HTTPS, or SOCKS5)
        #[arg(long)]
        proxy: Option<String>,

        /// Proxy URL for HTTP traffic (overrides --proxy for HTTP)
        #[arg(long)]
        proxy_http: Option<String>,

        /// Proxy URL for HTTPS traffic (overrides --proxy for HTTPS)
        #[arg(long)]
        proxy_https: Option<String>,

        /// Comma-separated list of hosts to bypass proxy
        #[arg(long)]
        no_proxy: Option<String>,

        /// Disable automatic loading of proxy settings from environment variables
        #[arg(long)]
        no_proxy_env: bool,
    },

    /// Start CDP WebSocket server for automation
    Serve {
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to listen on
        #[arg(long, default_value = "9222")]
        port: u16,

        /// Inactivity timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,

        /// Certificate pin (format: "sha256:HASH" or "host=DOMAIN:sha256:HASH")
        #[arg(long = "cert-pin")]
        cert_pin: Vec<String>,

        /// Load certificate pins from file (one per line, # comments)
        #[arg(long = "cert-pin-file")]
        cert_pin_file: Option<PathBuf>,

        /// Certificate pin match policy: require any or all pins to match
        #[arg(long = "pin-policy", value_enum)]
        pin_policy: Option<config::PinPolicyArg>,

        /// Proxy URL for all traffic (HTTP, HTTPS, or SOCKS5)
        #[arg(long)]
        proxy: Option<String>,

        /// Proxy URL for HTTP traffic (overrides --proxy for HTTP)
        #[arg(long)]
        proxy_http: Option<String>,

        /// Proxy URL for HTTPS traffic (overrides --proxy for HTTPS)
        #[arg(long)]
        proxy_https: Option<String>,

        /// Comma-separated list of hosts to bypass proxy
        #[arg(long)]
        no_proxy: Option<String>,

        /// Disable automatic loading of proxy settings from environment variables
        #[arg(long)]
        no_proxy_env: bool,
    },

    /// Start persistent interactive REPL session
    Repl {
        /// Enable JavaScript execution by default
        #[arg(long)]
        js: bool,

        /// Output format
        #[arg(short, long, default_value = "md")]
        format: OutputFormatArg,

        /// Wait time for JS execution (ms)
        #[arg(long, default_value = "3000")]
        wait_ms: u32,

        /// Proxy URL for all traffic (HTTP, HTTPS, or SOCKS5)
        #[arg(long)]
        proxy: Option<String>,

        /// Proxy URL for HTTP traffic (overrides --proxy for HTTP)
        #[arg(long)]
        proxy_http: Option<String>,

        /// Proxy URL for HTTPS traffic (overrides --proxy for HTTPS)
        #[arg(long)]
        proxy_https: Option<String>,

        /// Comma-separated list of hosts to bypass proxy
        #[arg(long)]
        no_proxy: Option<String>,

        /// Disable automatic loading of proxy settings from environment variables
        #[arg(long)]
        no_proxy_env: bool,
    },

    /// Tab management commands
    Tab {
        #[command(subcommand)]
        action: TabAction,

        /// Proxy URL for all traffic (HTTP, HTTPS, or SOCKS5)
        #[arg(long, global = true)]
        proxy: Option<String>,

        /// Proxy URL for HTTP traffic (overrides --proxy for HTTP)
        #[arg(long, global = true)]
        proxy_http: Option<String>,

        /// Proxy URL for HTTPS traffic (overrides --proxy for HTTPS)
        #[arg(long, global = true)]
        proxy_https: Option<String>,

        /// Comma-separated list of hosts to bypass proxy
        #[arg(long, global = true)]
        no_proxy: Option<String>,

        /// Disable automatic loading of proxy settings from environment variables
        #[arg(long, global = true)]
        no_proxy_env: bool,
    },

    /// Wipe all cache, cookies, and storage
    Clean {
        /// Clean specific directory
        #[arg(long)]
        cache_dir: Option<PathBuf>,

        /// Only clean cookies
        #[arg(long)]
        cookies_only: bool,

        /// Only clean cache
        #[arg(long)]
        cache_only: bool,
    },

    /// Map a site's functional structure into a Knowledge Graph
    Map {
        /// Root URL to start mapping from
        url: String,

        /// Output file path (JSON)
        #[arg(short, long, default_value = "kg.json")]
        output: PathBuf,

        /// Maximum crawl depth
        #[arg(short, long, default_value = "3")]
        depth: usize,

        /// Maximum pages to visit
        #[arg(long, default_value = "50")]
        max_pages: usize,

        /// Delay between requests (ms)
        #[arg(long, default_value = "200")]
        delay: u64,

        /// Skip transition verification
        #[arg(long)]
        skip_verify: bool,

        /// Discover pagination transitions
        #[arg(long, default_value = "true")]
        pagination: bool,

        /// Discover hash navigation
        #[arg(long, default_value = "true")]
        hash_nav: bool,

        /// Verbose logging
        #[arg(short, long)]
        verbose: bool,

        /// Proxy URL for all traffic (HTTP, HTTPS, or SOCKS5)
        #[arg(long)]
        proxy: Option<String>,

        /// Proxy URL for HTTP traffic (overrides --proxy for HTTP)
        #[arg(long)]
        proxy_http: Option<String>,

        /// Proxy URL for HTTPS traffic (overrides --proxy for HTTPS)
        #[arg(long)]
        proxy_https: Option<String>,

        /// Comma-separated list of hosts to bypass proxy
        #[arg(long)]
        no_proxy: Option<String>,

        /// Disable automatic loading of proxy settings from environment variables
        #[arg(long)]
        no_proxy_env: bool,
    },

    /// Capture a screenshot of a web page
    #[cfg(feature = "screenshot")]
    Screenshot {
        /// URL to capture
        url: String,

        /// Output file path
        #[arg(short, long, default_value = "screenshot.png")]
        output: PathBuf,

        /// Capture a specific element by CSS selector
        #[arg(long)]
        element: Option<String>,

        /// Capture the full page (scrolls to capture everything)
        #[arg(long)]
        full_page: bool,

        /// Viewport size (e.g., "1920x1080")
        #[arg(long, default_value = "1280x720")]
        viewport: String,

        /// Output format (png, jpeg)
        #[arg(long, default_value = "png")]
        format: String,

        /// JPEG quality (1-100, default: 85)
        #[arg(long)]
        quality: Option<u8>,

        /// Path to Chrome/Chromium binary
        #[arg(long)]
        chrome_path: Option<PathBuf>,

        /// Navigation timeout in milliseconds
        #[arg(long, default_value = "10000")]
        timeout_ms: u64,
    },
}

#[derive(Clone, Subcommand)]
pub enum InteractAction {
    /// Click on an element using CSS selector
    Click {
        /// CSS selector of the element to click
        selector: String,
    },

    /// Click on an element using its element ID (e.g., 1, 2, 3)
    ClickId {
        /// Element ID shown in the semantic tree (e.g., 1, 2, 3)
        id: usize,
    },

    /// Type text into a field
    Type {
        /// CSS selector of the field
        selector: String,
        /// Value to type
        value: String,
    },

    /// Type text into a field using its element ID
    TypeId {
        /// Element ID shown in the semantic tree
        id: usize,
        /// Value to type
        value: String,
    },

    /// Submit a form
    Submit {
        /// CSS selector of the form
        selector: String,
        /// Field values as "name=value" pairs
        #[arg(long)]
        field: Vec<String>,
    },

    /// Wait for a selector to appear
    Wait {
        /// CSS selector to wait for
        selector: String,
        /// Timeout in milliseconds
        #[arg(long, default_value = "5000")]
        timeout_ms: u32,
    },

    /// Scroll the page
    Scroll {
        /// Direction (down, up, to-top, to-bottom)
        #[arg(long, default_value = "down")]
        direction: String,
    },

    /// Dispatch an arbitrary DOM event on an element
    DispatchEvent {
        /// CSS selector of the target element
        selector: String,
        /// Event type (e.g., change, input, focus, blur, submit, custom)
        event_type: String,
        /// Event init options as JSON (e.g., {"bubbles":true,"detail":{}})
        #[arg(long)]
        init: Option<String>,
    },

    /// Upload files to a file input element
    Upload {
        /// CSS selector of the file input
        selector: String,
        /// File paths to upload
        #[arg(long, num_args = 1..)]
        files: Vec<String>,
    },

    /// Upload files to a file input by element ID
    UploadId {
        /// Element ID shown in the semantic tree
        id: usize,
        /// File paths to upload
        #[arg(long, num_args = 1..)]
        files: Vec<String>,
    },
}

#[derive(Clone, Debug, ValueEnum)]
pub enum OutputFormatArg {
    Md,
    Tree,
    Json,
    Llm,
}

#[derive(Clone, Subcommand)]
pub enum TabAction {
    /// List all open tabs
    List,
    /// Open a new tab with a URL
    Open {
        /// URL to open
        url: String,
        /// Enable JavaScript execution
        #[arg(long)]
        js: bool,
    },
    /// Show active tab info
    Info,
    /// Navigate active tab to a new URL
    Navigate {
        /// URL to navigate to
        url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Navigate {
            url,
            format,
            interactive_only,
            wait_ms,
            with_nav,
            persistent: _,
            header: _,
            js,
            verbose,
            network_log,
            cert_pin,
            cert_pin_file,
            pin_policy,
            proxy,
            proxy_http,
            proxy_https,
            no_proxy,
            no_proxy_env,
            har,
            coverage,
        } => {
            if verbose {
                tracing_subscriber::fmt()
                    .with_env_filter("pardus_core=debug")
                    .init();
            }

            let mut browser_config = pardus_core::BrowserConfig::default();
            browser_config.proxy =
                build_proxy_config(proxy, proxy_http, proxy_https, no_proxy, no_proxy_env);
            apply_cert_pinning(&mut browser_config, cert_pin, cert_pin_file, pin_policy);

            commands::navigate::run_with_config(
                &url,
                format,
                interactive_only,
                with_nav,
                js,
                wait_ms,
                network_log,
                har,
                coverage,
                browser_config,
            )
            .await?;
        }
        Commands::Interact {
            url,
            action,
            format,
            js,
            wait_ms,
            proxy,
            proxy_http,
            proxy_https,
            no_proxy,
            no_proxy_env,
        } => {
            let mut browser_config = pardus_core::BrowserConfig::default();
            browser_config.proxy =
                build_proxy_config(proxy, proxy_http, proxy_https, no_proxy, no_proxy_env);

            commands::interact::run_with_config(&url, action, format, js, wait_ms, browser_config)
                .await?;
        }
        Commands::Serve {
            host,
            port,
            timeout,
            cert_pin,
            cert_pin_file,
            pin_policy,
            proxy,
            proxy_http,
            proxy_https,
            no_proxy,
            no_proxy_env,
        } => {
            tracing::info!("Starting CDP WebSocket server on ws://{host}:{port}");

            let mut browser_config = pardus_core::BrowserConfig::default();
            browser_config.proxy =
                build_proxy_config(proxy, proxy_http, proxy_https, no_proxy, no_proxy_env);
            apply_cert_pinning(&mut browser_config, cert_pin, cert_pin_file, pin_policy);

            commands::serve::run(&host, port, timeout, browser_config).await?;
        }
        Commands::Clean {
            cache_dir,
            cookies_only,
            cache_only,
        } => {
            commands::clean::run(cache_dir, cookies_only, cache_only)?;
        }
        Commands::Tab {
            action,
            proxy,
            proxy_http,
            proxy_https,
            no_proxy,
            no_proxy_env,
        } => {
            let proxy_config =
                build_proxy_config(proxy, proxy_http, proxy_https, no_proxy, no_proxy_env);

            match action {
                TabAction::List => {
                    let mut browser_config = pardus_core::BrowserConfig::default();
                    browser_config.proxy = proxy_config;
                    let browser = pardus_core::Browser::new(browser_config)?;
                    commands::tab::list(&browser, OutputFormatArg::Md).await?;
                }
                TabAction::Open { url, js } => {
                    commands::tab::open_with_config(&url, js, proxy_config).await?;
                }
                TabAction::Info => {
                    let mut browser_config = pardus_core::BrowserConfig::default();
                    browser_config.proxy = proxy_config;
                    let browser = pardus_core::Browser::new(browser_config)?;
                    commands::tab::info(&browser, OutputFormatArg::Md)?;
                }
                TabAction::Navigate { url } => {
                    commands::tab::navigate_with_config(&url, proxy_config).await?;
                }
            }
        }
        Commands::Repl {
            js,
            format,
            wait_ms,
            proxy,
            proxy_http,
            proxy_https,
            no_proxy,
            no_proxy_env,
        } => {
            let proxy_config =
                build_proxy_config(proxy, proxy_http, proxy_https, no_proxy, no_proxy_env);

            commands::repl::run_with_config(js, format, wait_ms, proxy_config).await?;
        }
        Commands::Map {
            url,
            output,
            depth,
            max_pages,
            delay,
            skip_verify,
            pagination,
            hash_nav,
            verbose,
            proxy,
            proxy_http,
            proxy_https,
            no_proxy,
            no_proxy_env,
        } => {
            let proxy_config =
                build_proxy_config(proxy, proxy_http, proxy_https, no_proxy, no_proxy_env);

            commands::map::run_with_config(
                &url,
                &output,
                depth,
                max_pages,
                delay,
                skip_verify,
                pagination,
                hash_nav,
                verbose,
                proxy_config,
            )
            .await?;
        }
        #[cfg(feature = "screenshot")]
        Commands::Screenshot {
            url,
            output,
            element,
            full_page,
            viewport,
            format,
            quality,
            chrome_path,
            timeout_ms,
        } => {
            commands::screenshot::run(
                &url,
                &output,
                element.as_deref(),
                full_page,
                &viewport,
                &format,
                quality,
                chrome_path.as_ref(),
                timeout_ms,
            )
            .await?;
        }
    }

    Ok(())
}
