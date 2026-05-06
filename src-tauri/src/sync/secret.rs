//! OS-native secrets store wrapper for the GitHub PAT used by the sync
//! feature. The token never goes to a plaintext file: macOS Keychain,
//! Windows Credential Manager, or libsecret on Linux. Each platform's
//! store is per-user, so two operators on the same machine keep
//! their tokens isolated by OS user account.

const SERVICE: &str = "dmx-control.sync";
const ACCOUNT: &str = "github-pat";

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
}

pub fn save_token(token: &str) -> Result<(), String> {
    entry()?.set_password(token).map_err(|e| e.to_string())
}

/// Returns `Ok(None)` when the user simply hasn't paired the
/// machine yet (no entry under this service/account); only a real
/// keyring failure surfaces as `Err`.
pub fn load_token() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn clear_token() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn has_token() -> bool {
    load_token().map(|t| t.is_some()).unwrap_or(false)
}
