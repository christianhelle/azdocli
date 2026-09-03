//! Resolving identity arguments supplied on the command line.
//!
//! Reviewer and filter arguments accept a GUID, an email address, or `@me` for
//! the signed-in user, so every command that takes one resolves it here.

use super::http;
use crate::auth::Credentials;
use anyhow::Result;

/// The value callers pass to mean "the user this PAT belongs to".
const SELF: &str = "@me";

/// Resolves an identity argument to an Azure DevOps identity ID.
pub(super) async fn resolve_identity(creds: &Credentials, value: &str) -> Result<String> {
    let value = value.trim();

    if value.eq_ignore_ascii_case(SELF) {
        return http::authenticated_user_id(creds).await;
    }

    if is_guid(value) {
        return Ok(value.to_string());
    }

    crate::user::find_identity_id_by_email(creds, value).await
}

/// Reports whether a value has the shape of a GUID, in which case it is already
/// an identity ID and needs no lookup.
fn is_guid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }

    bytes.iter().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            *byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_guid_accepts_a_well_formed_guid() {
        assert!(is_guid("3b3f0a1c-9d2e-4f5a-8b6c-7d8e9f0a1b2c"));
    }

    #[test]
    fn is_guid_accepts_uppercase_hex() {
        assert!(is_guid("3B3F0A1C-9D2E-4F5A-8B6C-7D8E9F0A1B2C"));
    }

    #[test]
    fn is_guid_rejects_email_addresses() {
        assert!(!is_guid("someone@example.com"));
    }

    #[test]
    fn is_guid_rejects_wrong_length() {
        assert!(!is_guid("3b3f0a1c-9d2e-4f5a-8b6c-7d8e9f0a1b2"));
    }

    #[test]
    fn is_guid_rejects_misplaced_separators() {
        assert!(!is_guid("3b3f0a1c9-d2e-4f5a-8b6c-7d8e9f0a1b2c"));
    }
}
