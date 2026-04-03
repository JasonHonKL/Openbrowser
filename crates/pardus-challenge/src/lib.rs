//! CAPTCHA / bot-challenge detection and human-in-the-loop resolution.
//!
//! This crate provides:
//! - [`ChallengeDetector`] — inspects HTTP responses and HTML for CAPTCHA /
//!   challenge indicators (reCAPTCHA, hCaptcha, Cloudflare Turnstile, DataDome,
//!   PerimeterX, Akamai, etc.).
//! - [`ChallengeInterceptor`] — a [`pardus_core::intercept::Interceptor`] that
//!   pauses the pipeline when a challenge is detected, delegating resolution to
//!   a user-supplied [`ChallengeResolver`].
//! - [`ChallengeResolver`] — a trait that the host application (e.g. a Tauri
//!   desktop app) implements to present the challenge to a human and return
//!   cookies / headers once solved.

pub mod detector;
pub mod interceptor;
pub mod resolver;

pub use detector::{ChallengeDetector, ChallengeInfo, ChallengeKind};
pub use interceptor::ChallengeInterceptor;
pub use resolver::{ChallengeResolver, Resolution};
