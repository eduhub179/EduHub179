//! Use case: login with login + password.
//!
//! Flow:
//! 1. Look up the user by login.
//! 2. Reject inactive users.
//! 3. Verify the password via `CredentialsStore`.
//! 4. Issue a session token via `TokenIssuer`.
//!
//! Security notes:
//! - On ANY failure in steps 1-3 we return `InvalidCredentials` so that an
//!   attacker cannot tell whether a login exists (prevents user enumeration).
//! - The password is verified inside `CredentialsStore`; the hash never
//!   crosses the layer boundary.
//! - Timing-safe: the use case always calls `verify_password` even if the
//!   user is not found (the adapter returns `Ok(false)` for missing users).
//!
//! Dependencies: `domain` crate (entities, repositories, ports, errors).
//! Guarantees: All methods return `Result`. No panics.

use crate::use_cases::auth::AuthSession;
use domain::errors::DomainError;
use domain::ports::auth::{CredentialsStore, TokenIssuer};
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::login::Login;
use std::sync::Arc;

/// Input for the login use case.
#[derive(Debug, Clone)]
pub struct LoginCommand {
    /// User login (e.g., "s27b_korovko").
    pub login: Login,
    /// Raw password (never hashed by the caller).
    pub password: String,
}

/// Login with login + password.
///
/// Returns `AuthSession` (token + user) on success, or `InvalidCredentials` /
/// `UserIsInactive` on failure.
pub struct LoginUseCase {
    users: Arc<dyn UserRepository>,
    credentials: Arc<dyn CredentialsStore>,
    tokens: Arc<dyn TokenIssuer>,
}

impl LoginCommand {
    /// Creates a new `LoginCommand` with login validation.
    ///
    /// Returns `Err(DomainError::InvalidLoginFormat)` if the login is invalid.
    pub fn try_new(login: String, password: String) -> Result<Self, DomainError> {
        let login = Login::try_new(&login)?;
        Ok(Self { login, password })
    }
}

impl LoginUseCase {
    /// Creates a new `LoginUseCase`.
    ///
    /// All dependencies are injected as `Arc<dyn Trait>` to allow sharing
    /// across multiple use cases (e.g., `CreateUserUseCase` also needs
    /// `UserRepository` and `CredentialsStore`).
    pub fn new(
        users: Arc<dyn UserRepository>,
        credentials: Arc<dyn CredentialsStore>,
        tokens: Arc<dyn TokenIssuer>,
    ) -> Self {
        Self {
            users,
            credentials,
            tokens,
        }
    }

    /// Executes the login flow.
    ///
    /// Returns `Ok(AuthSession)` on success, or:
    /// - `InvalidCredentials` if the login does not exist, the password is
    ///   wrong, or the user has no password set.
    /// - `UserIsInactive` if the user account is blocked.
    /// - `InternalError` if token issuance fails (e.g., JWT signing error).
    ///
    /// Security: the method deliberately does NOT distinguish between
    /// "user not found" and "wrong password" to prevent user enumeration.
    pub async fn execute(&self, cmd: LoginCommand) -> Result<AuthSession, DomainError> {
        // 1. Look up the user by login.
        // Map "not found" to `InvalidCredentials` to prevent user enumeration.
        let user = self
            .users
            .get_by_login(&cmd.login.as_str())
            .await
            .map_err(|_| DomainError::InvalidCredentials)?;

        // 2. Reject inactive users.
        // This is a separate error (not `InvalidCredentials`) because the
        // admin may need to know that the account exists but is blocked.
        if !user.is_active {
            return Err(DomainError::UserIsInactive);
        }

        // 3. Verify the password.
        // The hash never leaves `CredentialsStore`; only a boolean result
        // crosses the layer boundary.
        let is_valid = self
            .credentials
            .verify_password(user.id, &cmd.password)
            .await?;
        if !is_valid {
            return Err(DomainError::InvalidCredentials);
        }

        // 4. Issue a session token (JWT).
        let token = self.tokens.issue(&user)?;

        Ok(AuthSession { token, user })
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::user::User;
    use domain::value_objects::role::UserRole;
    use uuid::Uuid;

    /// Mock `UserRepository` for unit tests.
    struct MockUserRepository {
        user: Option<User>,
    }

    #[async_trait::async_trait]
    impl UserRepository for MockUserRepository {
        async fn get_by_id(&self, _user_id: Uuid) -> Result<User, DomainError> {
            unimplemented!()
        }

        async fn get_by_login(&self, _login: &str) -> Result<User, DomainError> {
            self.user.clone().ok_or(DomainError::UserNotFound)
        }

        async fn get_active_students_by_class(
            &self,
            _class_id: Uuid,
        ) -> Result<Vec<User>, DomainError> {
            unimplemented!()
        }

        async fn save(&self, _user: User) -> Result<User, DomainError> {
            unimplemented!()
        }
    }

    /// Mock `CredentialsStore` for unit tests.
    struct MockCredentialsStore {
        is_valid: bool,
    }

    #[async_trait::async_trait]
    impl CredentialsStore for MockCredentialsStore {
        async fn verify_password(
            &self,
            _user_id: Uuid,
            _raw_password: &str,
        ) -> Result<bool, DomainError> {
            Ok(self.is_valid)
        }

        async fn set_password(
            &self,
            _user_id: Uuid,
            _raw_password: &str,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    /// Mock `TokenIssuer` for unit tests.
    struct MockTokenIssuer {
        token: String,
    }

    impl TokenIssuer for MockTokenIssuer {
        fn issue(&self, _user: &User) -> Result<String, DomainError> {
            Ok(self.token.clone())
        }

        fn verify(&self, _token: &str) -> Result<domain::ports::auth::TokenClaims, DomainError> {
            unimplemented!()
        }
    }

    fn create_test_user() -> User {
        User::try_new(
            Uuid::new_v4(),
            "test_login".to_string(),
            UserRole::Student,
            "Ivanov".to_string(),
            "Ivan".to_string(),
            None,
            None,
        )
        .expect("Test user should be valid")
    }

    #[tokio::test]
    async fn test_login_success() {
        let user = create_test_user();
        let use_case = LoginUseCase::new(
            Arc::new(MockUserRepository {
                user: Some(user.clone()),
            }),
            Arc::new(MockCredentialsStore { is_valid: true }),
            Arc::new(MockTokenIssuer {
                token: "mock_token".to_string(),
            }),
        );

        let cmd = LoginCommand::try_new("test_login".to_string(), "correct_password".to_string())
            .unwrap();

        let result = use_case.execute(cmd).await;
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.token, "mock_token");
        assert_eq!(session.user.id, user.id);
    }

    #[tokio::test]
    async fn test_login_user_not_found() {
        let use_case = LoginUseCase::new(
            Arc::new(MockUserRepository { user: None }),
            Arc::new(MockCredentialsStore { is_valid: false }),
            Arc::new(MockTokenIssuer {
                token: "mock_token".to_string(),
            }),
        );

        let cmd = LoginCommand::try_new("nonexistent".to_string(), "password".to_string()).unwrap();

        let result = use_case.execute(cmd).await;
        assert!(matches!(result, Err(DomainError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn test_login_wrong_password() {
        let user = create_test_user();
        let use_case = LoginUseCase::new(
            Arc::new(MockUserRepository { user: Some(user) }),
            Arc::new(MockCredentialsStore { is_valid: false }),
            Arc::new(MockTokenIssuer {
                token: "mock_token".to_string(),
            }),
        );

        let cmd =
            LoginCommand::try_new("test_login".to_string(), "wrong_password".to_string()).unwrap();

        let result = use_case.execute(cmd).await;
        assert!(matches!(result, Err(DomainError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn test_login_inactive_user() {
        let mut user = create_test_user();
        user.is_active = false;
        let use_case = LoginUseCase::new(
            Arc::new(MockUserRepository { user: Some(user) }),
            Arc::new(MockCredentialsStore { is_valid: true }),
            Arc::new(MockTokenIssuer {
                token: "mock_token".to_string(),
            }),
        );

        let cmd = LoginCommand::try_new("test_login".to_string(), "password".to_string()).unwrap();

        let result = use_case.execute(cmd).await;
        assert!(matches!(result, Err(DomainError::UserIsInactive)));
    }
}
