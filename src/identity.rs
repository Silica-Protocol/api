use std::borrow::Cow;

use anyhow::{Result, anyhow};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

pub const IDENTITY_ID_BYTES: usize = 32;
pub const AVATAR_HASH_BYTES: usize = 32;
pub const MAX_DISPLAY_NAME_LEN: usize = 64;
pub const MAX_BIO_LEN: usize = 280;
pub const MAX_WALLET_ADDRESS_LEN: usize = 128;
pub const MAX_WALLET_LINKS: usize = 32;
pub const MAX_SIGNATURE_LEN: usize = 4096;

const _: [(); 16_384 - MAX_SIGNATURE_LEN] = [(); 16_384 - MAX_SIGNATURE_LEN];
const _: [(); 64 - MAX_WALLET_LINKS] = [(); 64 - MAX_WALLET_LINKS];

pub const VISIBILITY_PUBLIC: &str = "public";
pub const VISIBILITY_FRIENDS_ONLY: &str = "friends_only";
pub const VISIBILITY_PRIVATE: &str = "private";

pub fn decode_identity_id(value: &str) -> Result<Vec<u8>> {
    let bytes = decode_hex_with_expected(value, IDENTITY_ID_BYTES, "identity id")?;
    Ok(bytes)
}

pub fn encode_identity_id(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

pub fn decode_hex_with_expected(value: &str, expected_len: usize, label: &str) -> Result<Vec<u8>> {
    assert!(expected_len > 0, "Expected length must be > 0");
    assert!(
        expected_len <= 4096,
        "Expected length exceeds defensive bound"
    );
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("{label} cannot be empty"));
    }
    let normalized = strip_hex_prefix(trimmed);
    let bytes =
        hex::decode(normalized).map_err(|err| anyhow!("Failed to decode {label} as hex: {err}"))?;
    if bytes.len() != expected_len {
        return Err(anyhow!(
            "{label} must be {expected_len} bytes, got {}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

pub fn decode_signature(value: &str) -> Result<Vec<u8>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Signature cannot be empty"));
    }

    match hex::decode(strip_hex_prefix(trimmed)) {
        Ok(bytes) if !bytes.is_empty() => {
            if bytes.len() > MAX_SIGNATURE_LEN {
                return Err(anyhow!(
                    "Signature exceeds {MAX_SIGNATURE_LEN} byte defensive limit"
                ));
            }
            return Ok(bytes);
        }
        Ok(_) => {}
        Err(_) => {}
    }

    let decoded = BASE64_STANDARD
        .decode(trimmed)
        .map_err(|err| anyhow!("Failed to decode signature as hex or base64: {err}"))?;
    if decoded.len() > MAX_SIGNATURE_LEN {
        return Err(anyhow!(
            "Signature exceeds {MAX_SIGNATURE_LEN} byte defensive limit"
        ));
    }
    Ok(decoded)
}

pub fn canonicalize_display_name(value: &str) -> Result<Option<String>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_DISPLAY_NAME_LEN {
        return Err(anyhow!(
            "Display name exceeds {MAX_DISPLAY_NAME_LEN} character limit"
        ));
    }
    Ok(Some(trimmed.to_string()))
}

pub fn canonicalize_bio(value: &str) -> Result<Option<String>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_BIO_LEN {
        return Err(anyhow!("Bio exceeds {MAX_BIO_LEN} character limit"));
    }
    Ok(Some(trimmed.to_string()))
}

pub fn display_name_search_key(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

pub fn normalize_visibility(value: &str) -> Result<&'static str> {
    let normalized = value.trim().to_ascii_lowercase();
    let visibility = match normalized.as_str() {
        VISIBILITY_PUBLIC => VISIBILITY_PUBLIC,
        VISIBILITY_FRIENDS_ONLY => VISIBILITY_FRIENDS_ONLY,
        VISIBILITY_PRIVATE => VISIBILITY_PRIVATE,
        other => {
            return Err(anyhow!("Unsupported stats visibility value: {other}"));
        }
    };
    Ok(visibility)
}

pub fn normalize_link_type(value: &str) -> Result<Cow<'static, str>> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(anyhow!("Wallet link type cannot be empty"));
    }
    if normalized.len() > 32 {
        return Err(anyhow!("Wallet link type exceeds 32 character limit"));
    }
    let link_type = match normalized.as_str() {
        "main" | "primary" => Cow::Borrowed("main"),
        "mining" => Cow::Borrowed("mining"),
        "staking" => Cow::Borrowed("staking"),
        "trading" => Cow::Borrowed("trading"),
        "governance" => Cow::Borrowed("governance"),
        other => Cow::Owned(other.to_string()),
    };
    Ok(link_type)
}

pub fn sanitize_wallet_address(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Wallet address cannot be empty"));
    }
    if trimmed.len() > MAX_WALLET_ADDRESS_LEN {
        return Err(anyhow!(
            "Wallet address exceeds {MAX_WALLET_ADDRESS_LEN} character limit"
        ));
    }
    Ok(trimmed.to_string())
}

fn strip_hex_prefix(value: &str) -> &str {
    if value.starts_with("0x") || value.starts_with("0X") {
        &value[2..]
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_roundtrip() {
        let id = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let decoded = decode_identity_id(id).expect("decode succeeds");
        assert_eq!(decoded.len(), IDENTITY_ID_BYTES);
        assert_eq!(encode_identity_id(&decoded), strip_hex_prefix(id));
    }

    #[test]
    fn signature_decodes_hex_and_base64() {
        let hex_encoded = "0xdeadbeef";
        let hex_bytes = decode_signature(hex_encoded).expect("hex signature");
        assert_eq!(hex_bytes, vec![0xde, 0xad, 0xbe, 0xef]);

        let base64_encoded = BASE64_STANDARD.encode([0xde, 0xad, 0xbe, 0xef]);
        let base64_bytes = decode_signature(&base64_encoded).expect("base64 signature");
        assert_eq!(base64_bytes, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn display_name_validation() {
        assert!(canonicalize_display_name("Alice").unwrap().is_some());
        let long_name = "a".repeat(MAX_DISPLAY_NAME_LEN + 1);
        assert!(canonicalize_display_name(&long_name).is_err());
    }

    #[test]
    fn bio_validation() {
        let long_bio = "x".repeat(MAX_BIO_LEN + 1);
        assert!(canonicalize_bio(&long_bio).is_err());
    }

    #[test]
    fn visibility_normalization() {
        assert_eq!(normalize_visibility("PUBLIC").unwrap(), VISIBILITY_PUBLIC);
        assert!(normalize_visibility("secret").is_err());
    }

    #[test]
    fn link_type_normalization() {
        assert_eq!(normalize_link_type("Main").unwrap(), "main");
        assert_eq!(normalize_link_type("Custom").unwrap(), "custom");
        assert!(normalize_link_type("").is_err());
    }

    #[test]
    fn wallet_address_validation() {
        assert!(sanitize_wallet_address("0xabc").is_ok());
        let too_long = "a".repeat(MAX_WALLET_ADDRESS_LEN + 1);
        assert!(sanitize_wallet_address(&too_long).is_err());
    }
}
