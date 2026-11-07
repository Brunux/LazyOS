//! Configuration data structures, validation rules, and serialization for the LazyOS installer.

use serde::{Deserialize, Serialize};

/// Target configuration for a complete LazyOS installation onto storage.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallConfig {
    /// Full display name of the primary user (e.g., "Bruno").
    pub fullname: String,
    /// POSIX username for system login and home directory (e.g., "brunux").
    pub username: String,
    /// Plaintext password to set on the target user account.
    pub password: String,
    /// Confirmation password for interactive validation.
    pub password_confirm: String,
    /// System hostname (default "lazyos").
    pub hostname: String,
    /// Selected IANA timezone (default "UTC" or system-detected).
    pub timezone: String,
    /// Selected system locale (default "en_US.UTF-8").
    pub locale: String,
    /// Selected console and X11 keyboard layout (default "us").
    pub keymap: String,
    /// Whether to erase inherited machine identity and generate fresh SSH host keys.
    pub regenerate_ssh_keys: bool,
    /// Target block device path (default "/dev/mmcblk0").
    pub target_disk: String,
    /// Whether to run in non-destructive dry-run simulation mode.
    pub dry_run: bool,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            fullname: "LazyOS User".to_string(),
            username: "brunux".to_string(),
            password: String::new(),
            password_confirm: String::new(),
            hostname: "lazyos".to_string(),
            timezone: "UTC".to_string(),
            locale: "en_US.UTF-8".to_string(),
            keymap: "us".to_string(),
            regenerate_ssh_keys: true,
            target_disk: "/dev/mmcblk0".to_string(),
            dry_run: false,
        }
    }
}

impl InstallConfig {
    /// Validate all user credential and hostname fields.
    pub fn validate_user_fields(&self) -> Result<(), &'static str> {
        validate_username(&self.username)?;
        validate_password(&self.password, &self.password_confirm)?;
        validate_hostname(&self.hostname)?;
        Ok(())
    }
}

/// Validate username against strict POSIX rules: `^[a-z_][a-z0-9_-]{0,31}$`.
/// Rejects empty usernames, root, and illegal characters.
pub fn validate_username(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("Username cannot be empty");
    }
    if name.len() > 32 {
        return Err("Username must be at most 32 characters");
    }
    if name == "root" {
        return Err("Username 'root' is reserved");
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() && first != '_' {
        return Err("Username must start with a lowercase letter or underscore");
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' && c != '-' {
            return Err(
                "Username can only contain lowercase letters, numbers, hyphens, and underscores",
            );
        }
    }
    Ok(())
}

/// Validate that password is non-empty and matches the confirmation.
pub fn validate_password(p1: &str, p2: &str) -> Result<(), &'static str> {
    if p1.is_empty() {
        return Err("Password cannot be empty");
    }
    if p1 != p2 {
        return Err("Passwords do not match");
    }
    Ok(())
}

/// Validate system hostname syntax: alphanumeric and hyphens, 1-63 chars, no leading/trailing hyphen.
pub fn validate_hostname(host: &str) -> Result<(), &'static str> {
    if host.is_empty() {
        return Err("Hostname cannot be empty");
    }
    if host.len() > 63 {
        return Err("Hostname must be at most 63 characters");
    }
    if host.starts_with('-') || host.ends_with('-') {
        return Err("Hostname cannot start or end with a hyphen");
    }
    for c in host.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' {
            return Err("Hostname can only contain letters, numbers, and hyphens");
        }
    }
    Ok(())
}

/// Validate typed confirmation string against target disk name or explicit DESTROY keyword.
pub fn validate_confirmation(input: &str, target_name: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed == target_name || trimmed.eq_ignore_ascii_case("DESTROY")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_validation_accepts_posix_identifiers() {
        assert!(validate_username("brunux").is_ok());
        assert!(validate_username("_service").is_ok());
        assert!(validate_username("user-123").is_ok());
        assert!(validate_username("u_456").is_ok());
    }

    #[test]
    fn username_validation_rejects_invalid_names() {
        assert_eq!(validate_username(""), Err("Username cannot be empty"));
        assert_eq!(
            validate_username("root"),
            Err("Username 'root' is reserved")
        );
        assert_eq!(
            validate_username("123user"),
            Err("Username must start with a lowercase letter or underscore")
        );
        assert_eq!(
            validate_username("Brunux"),
            Err("Username must start with a lowercase letter or underscore")
        );
        assert_eq!(
            validate_username("user name"),
            Err("Username can only contain lowercase letters, numbers, hyphens, and underscores")
        );
        assert_eq!(
            validate_username("user@home"),
            Err("Username can only contain lowercase letters, numbers, hyphens, and underscores")
        );
        let long = "a".repeat(33);
        assert_eq!(
            validate_username(&long),
            Err("Username must be at most 32 characters")
        );
    }

    #[test]
    fn password_matching_logic() {
        assert!(validate_password("secret123", "secret123").is_ok());
        assert_eq!(validate_password("", ""), Err("Password cannot be empty"));
        assert_eq!(
            validate_password("abc", "xyz"),
            Err("Passwords do not match")
        );
    }

    #[test]
    fn hostname_validation_rules() {
        assert!(validate_hostname("lazyos").is_ok());
        assert!(validate_hostname("chromebook-mt8192").is_ok());
        assert!(validate_hostname("host1").is_ok());
        assert_eq!(validate_hostname(""), Err("Hostname cannot be empty"));
        assert_eq!(
            validate_hostname("-invalid"),
            Err("Hostname cannot start or end with a hyphen")
        );
        assert_eq!(
            validate_hostname("invalid-"),
            Err("Hostname cannot start or end with a hyphen")
        );
        assert_eq!(
            validate_hostname("host.local"),
            Err("Hostname can only contain letters, numbers, and hyphens")
        );
        let long = "h".repeat(64);
        assert_eq!(
            validate_hostname(&long),
            Err("Hostname must be at most 63 characters")
        );
    }

    #[test]
    fn confirmation_gate_matching() {
        assert!(validate_confirmation("mmcblk0", "mmcblk0"));
        assert!(validate_confirmation("  mmcblk0  ", "mmcblk0"));
        assert!(validate_confirmation("DESTROY", "mmcblk0"));
        assert!(validate_confirmation("destroy", "mmcblk0"));
        assert!(!validate_confirmation("", "mmcblk0"));
        assert!(!validate_confirmation("sda", "mmcblk0"));
        assert!(!validate_confirmation("yes", "mmcblk0"));
    }
}
