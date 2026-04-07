use std::path::PathBuf;

use anyhow::Result;
use clap::ValueEnum;
use pardus_core::{CertPin, CertificatePinningConfig, PinAlgorithm, PinMatchPolicy};

#[derive(Clone, Debug, ValueEnum)]
pub enum PinPolicyArg {
    Any,
    All,
}

impl From<PinPolicyArg> for PinMatchPolicy {
    fn from(value: PinPolicyArg) -> Self {
        match value {
            PinPolicyArg::Any => PinMatchPolicy::RequireAny,
            PinPolicyArg::All => PinMatchPolicy::RequireAll,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum PinAlgoArg {
    Sha256,
    Sha384,
    Sha512,
}

impl From<PinAlgoArg> for PinAlgorithm {
    fn from(value: PinAlgoArg) -> Self {
        match value {
            PinAlgoArg::Sha256 => PinAlgorithm::Sha256,
            PinAlgoArg::Sha384 => PinAlgorithm::Sha384,
            PinAlgoArg::Sha512 => PinAlgorithm::Sha512,
        }
    }
}

/// Parse pin specifications from CLI arguments.
///
/// Formats:
///   - `sha256:BASE64_HASH` (SPKI hash pin)
///   - `sha384:BASE64_HASH`
///   - `sha512:BASE64_HASH`
///   - `ca:BASE64_DER`
///   - `host=example.com:sha256:HASH` (host-specific pin)
fn parse_pin_spec(spec: &str) -> Result<(Option<String>, CertPin)> {
    let (host, pin_str) = if let Some(rest) = spec.strip_prefix("host=") {
        if let Some(colon_pos) = rest.find(':') {
            (Some(rest[..colon_pos].to_string()), &rest[colon_pos + 1..])
        } else {
            anyhow::bail!(
                "invalid pin spec '{}': expected 'host=DOMAIN:TYPE:VALUE'",
                spec
            );
        }
    } else {
        (None, spec)
    };

    let pin = if let Some(rest) = pin_str.strip_prefix("sha256:") {
        CertPin::spki_sha256(rest)
    } else if let Some(rest) = pin_str.strip_prefix("sha384:") {
        CertPin::spki_sha384(rest)
    } else if let Some(rest) = pin_str.strip_prefix("sha512:") {
        CertPin::spki_sha512(rest)
    } else if let Some(rest) = pin_str.strip_prefix("ca:") {
        CertPin::ca_cert(rest, None)
    } else {
        anyhow::bail!(
            "invalid pin spec '{}': expected 'sha256:HASH', 'sha384:HASH', 'sha512:HASH', or \
             'ca:BASE64_DER'",
            spec
        );
    };

    Ok((host, pin))
}

pub fn build_cert_pinning_config(
    pins: &[String],
    policy: Option<PinPolicyArg>,
    enforce: bool,
) -> Result<CertificatePinningConfig> {
    let mut config = CertificatePinningConfig::new().enforce_on_failure(enforce);

    if let Some(p) = policy {
        config = config.with_policy(p.into());
    }

    for spec in pins {
        let (host, pin) = parse_pin_spec(spec)?;
        if let Some(host) = host {
            config = config.add_pin_for_host(&host, pin);
        } else {
            config = config.add_default_pin(pin);
        }
    }

    Ok(config)
}

pub fn load_pins_from_file(path: &PathBuf) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)?;
    let pins: Vec<String> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect();
    Ok(pins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_spki_sha256_pin() {
        let (host, pin) = parse_pin_spec("sha256:AAAAAAAAAAAAAAAAAAAAAA").unwrap();
        assert!(host.is_none());
        match pin {
            CertPin::SpkiHash { algorithm, hash } => {
                assert_eq!(algorithm, PinAlgorithm::Sha256);
                assert_eq!(hash, "AAAAAAAAAAAAAAAAAAAAAA");
            }
            _ => panic!("expected SpkiHash"),
        }
    }

    #[test]
    fn test_parse_host_specific_pin() {
        let (host, pin) = parse_pin_spec("host=example.com:sha256:hash123").unwrap();
        assert_eq!(host, Some("example.com".to_string()));
        match pin {
            CertPin::SpkiHash { algorithm, hash } => {
                assert_eq!(algorithm, PinAlgorithm::Sha256);
                assert_eq!(hash, "hash123");
            }
            _ => panic!("expected SpkiHash"),
        }
    }

    #[test]
    fn test_parse_ca_pin() {
        let (host, pin) = parse_pin_spec("ca:SGVsbG8=").unwrap();
        assert!(host.is_none());
        match pin {
            CertPin::CaCertificate { der_base64, .. } => {
                assert_eq!(der_base64, "SGVsbG8=");
            }
            _ => panic!("expected CaCertificate"),
        }
    }

    #[test]
    fn test_parse_invalid_pin() {
        assert!(parse_pin_spec("invalid").is_err());
    }

    #[test]
    fn test_build_config_from_cli() {
        let config = build_cert_pinning_config(
            &[
                "sha256:hash1".to_string(),
                "host=api.example.com:sha256:hash2".to_string(),
            ],
            Some(PinPolicyArg::All),
            true,
        )
        .unwrap();

        assert_eq!(config.policy, PinMatchPolicy::RequireAll);
        assert!(config.enforce);
        assert_eq!(config.get_pins_for_host("api.example.com").len(), 1);
        assert_eq!(config.get_pins_for_host("other.com").len(), 1);
    }
}
