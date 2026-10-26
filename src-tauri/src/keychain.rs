//! API keys in the OS keychain (spec section 13): Windows Credential Manager,
//! the Secret Service on Linux, Keychain Services on macOS.

use cv_app::{SecretError, SecretStore};
use keyring::{Entry, Error};

/// Service name the keys are filed under; the account is the provider.
const SERVICE: &str = "dev.pinaxcv.app";

pub struct Keychain;

fn entry(account: &str) -> Result<Entry, SecretError> {
    Entry::new(SERVICE, account).map_err(secret_error)
}

/// Keychain errors carry the kind only, never the secret.
fn secret_error(e: Error) -> SecretError {
    match e {
        Error::NoDefaultStore | Error::NoStorageAccess(_) => SecretError::Unavailable,
        Error::PlatformFailure(_) => SecretError::Failed("platform failure".into()),
        Error::TooLong(..) => SecretError::Failed("too long".into()),
        _ => SecretError::Failed("keychain error".into()),
    }
}

impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        match entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(secret_error(e)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        entry(account)?.set_password(secret).map_err(secret_error)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(secret_error(e)),
        }
    }
}
