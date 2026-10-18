//! Value Object for notification platform.
//!
//! Guarantees: only known push providers are accepted.

use crate::errors::DomainError;
use std::str::FromStr;

/// Platform used for receiving push notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationPlatform {
    Android,
    Ios,
    Web,
}

impl NotificationPlatform {
    /// Returns the provider name used in the database or in request DTOs.
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationPlatform::Android => "android",
            NotificationPlatform::Ios => "ios",
            NotificationPlatform::Web => "web",
        }
    }
}

impl FromStr for NotificationPlatform {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "android" => Ok(NotificationPlatform::Android),
            "ios" => Ok(NotificationPlatform::Ios),
            "web" => Ok(NotificationPlatform::Web),
            _ => Err(DomainError::InvalidDeviceTokenFormat),
        }
    }
}

impl std::fmt::Display for NotificationPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_platforms() {
        assert_eq!("android".parse::<NotificationPlatform>().unwrap(), NotificationPlatform::Android);
        assert_eq!("ios".parse::<NotificationPlatform>().unwrap(), NotificationPlatform::Ios);
        assert_eq!("web".parse::<NotificationPlatform>().unwrap(), NotificationPlatform::Web);
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!("ANDROID".parse::<NotificationPlatform>().unwrap(), NotificationPlatform::Android);
        assert_eq!("Ios".parse::<NotificationPlatform>().unwrap(), NotificationPlatform::Ios);
    }

    #[test]
    fn unknown_platform_is_rejected() {
        assert!("macos".parse::<NotificationPlatform>().is_err());
        assert!("".parse::<NotificationPlatform>().is_err());
    }
}
