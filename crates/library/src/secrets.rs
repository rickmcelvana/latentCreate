use keyring::Entry;

use crate::LibraryError;

/// Keychain service name. Shared by every latentCreate secret.
const SERVICE: &str = "latentCreate";

/// The secrets this app is allowed to store.
///
/// A closed set on purpose: the frontend names a secret by string, and without a
/// whitelist a compromised or buggy webview could write arbitrary entries into the
/// user's keychain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKey {
    /// API key for Comfy Cloud.
    ComfyCloudApiKey,
    /// API key for the lyric LLM provider.
    LlmApiKey,
}

impl SecretKey {
    /// Stable keychain account name. Changing one of these orphans existing entries.
    pub fn as_str(self) -> &'static str {
        match self {
            SecretKey::ComfyCloudApiKey => "comfy_cloud_api_key",
            SecretKey::LlmApiKey => "llm_api_key",
        }
    }

    /// Parses a name from the frontend, rejecting anything not in the whitelist.
    pub fn parse(name: &str) -> Result<Self, LibraryError> {
        match name {
            "comfy_cloud_api_key" => Ok(SecretKey::ComfyCloudApiKey),
            "llm_api_key" => Ok(SecretKey::LlmApiKey),
            other => Err(LibraryError::UnknownSecret(other.to_string())),
        }
    }
}

fn entry(key: SecretKey) -> Result<Entry, LibraryError> {
    Entry::new(SERVICE, key.as_str()).map_err(LibraryError::from)
}

/// Stores `value` in the OS keychain, replacing any existing entry.
pub fn set_secret(key: SecretKey, value: &str) -> Result<(), LibraryError> {
    entry(key)?.set_password(value).map_err(LibraryError::from)
}

/// Reads a secret.
///
/// **Never expose this through a Tauri command.** Secret *values* must not cross into
/// the webview: Rust reads them when it builds an outbound request, and the frontend
/// only ever learns whether one exists ([`has_secret`]).
pub fn get_secret(key: SecretKey) -> Result<String, LibraryError> {
    entry(key)?.get_password().map_err(LibraryError::from)
}

/// Whether a secret is stored. Any keychain error reads as "not stored" -- the caller
/// wants a UI checkmark, not an error path.
///
/// Note this *reads* the secret to answer, because the backends expose no cheaper
/// existence check. Two consequences: on macOS the first call for a given entry can
/// raise the system's keychain-access prompt, and the value is briefly in memory. So
/// call it when a screen loads, **never on every render or in a polling loop** -- a
/// permission dialog appearing repeatedly is worse than a missing checkmark.
pub fn has_secret(key: SecretKey) -> bool {
    entry(key)
        .and_then(|e| e.get_password().map_err(LibraryError::from))
        .is_ok()
}

/// Removes a secret. Deleting one that does not exist is not an error.
pub fn delete_secret(key: SecretKey) -> Result<(), LibraryError> {
    match entry(key)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(LibraryError::from(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rejects_unknown_secret_name() {
        assert!(matches!(
            SecretKey::parse("../../etc/passwd"),
            Err(LibraryError::UnknownSecret(_))
        ));
        assert!(matches!(
            SecretKey::parse("arbitrary"),
            Err(LibraryError::UnknownSecret(_))
        ));
    }

    #[test]
    fn test_secret_key_names_are_stable() {
        assert_eq!(SecretKey::ComfyCloudApiKey.as_str(), "comfy_cloud_api_key");
        assert_eq!(SecretKey::LlmApiKey.as_str(), "llm_api_key");
    }

    #[test]
    #[ignore = "touches the OS keychain; run manually with cargo test -p library -- --ignored"]
    fn test_set_get_delete_roundtrip() {
        let key = SecretKey::LlmApiKey;
        set_secret(key, "test-value").unwrap();
        assert_eq!(get_secret(key).unwrap(), "test-value");
        delete_secret(key).unwrap();
        assert!(!has_secret(key));
    }
}
