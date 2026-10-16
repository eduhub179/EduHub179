use std::sync::Arc;

use application::use_cases::auth::login::LoginUseCase;
use infrastructure::{
    auth::{Argon2PasswordHasher, JwtTokenIssuer},
    config::Config,
    postgres::{create_pool, CredentialsStorePg, UserRepositoryPg},
};
use presentation::create_app_with_auth;

#[tokio::main]
async fn main() {
    let config = Config::load();

    let pool = create_pool(&config.database_url)
        .await
        .expect("failed to create PostgreSQL pool");

    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let password_hasher = Arc::new(Argon2PasswordHasher);
    let credentials_store = Arc::new(CredentialsStorePg::new(pool.clone(), password_hasher));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        config.jwt_secret.clone(),
        config.jwt_ttl_seconds as i64,
    ));

    let login_use_case = Arc::new(LoginUseCase::new(user_repo, credentials_store, token_issuer));

    let app = create_app_with_auth(login_use_case);
    let addr = format!("{}:{}", config.server_host, config.server_port);

    println!("starting EduHub179 API on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind server port");

    axum::serve(listener, app)
        .await
        .expect("server failed");
}
