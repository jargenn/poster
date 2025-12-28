// Based on https://developers.facebook.com/docs/facebook-login/guides/advanced/manual-flow, (2025-12-26)

use std::{
    collections::HashMap,
    ops::Deref,
    str::FromStr as _,
    time::{Duration, SystemTime},
};

use crate::{
    AppState,
    cookies::{SessionId, build_session_cookie},
    db::{load_session, store_session},
};
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use eyre::Result;
use reqwest::{Client, StatusCode, header};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument};
use url::Url;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum AuthError {
    #[error("The access token you are using doesnt belong to the this App")]
    WrongApp,
    #[error("Provided access token is no longer valid")]
    InvalidToken,
    #[error("You are not logged in to the App")]
    NotLoggedIn,
    #[error("Access token has expired")]
    Expired,
}

// /// TODO: Search for what scopes are there.
// #[derive(Debug, Serialize, Deserialize)]
// #[serde(rename_all = "snake_case")]
// enum Scope {
//     Email,
//     PublishActions,
// }

#[derive(Debug, Serialize, Deserialize)]
struct DebugResponseMetadata {
    /// TODO: Research what does this metadata contain
    sso: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DebugAccessTokenData {
    app_id: String,
    #[serde(rename = "type")]
    token_type: String,
    application: String,
    expires_at: Option<u64>,
    is_valid: bool,
    issued_at: String,
    metadata: DebugResponseMetadata,
    // TODO: Model it properly
    scopes: Vec<String>,
    user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DebugAccessTokenResponse {
    pub data: DebugAccessTokenData,
}

// TODO: Completar con lo que dice la doc de Facebook
#[derive(Debug)]
pub struct Redirected {
    pub code: String,
    pub redirect_uri: RedirectUri,
    pub csrf_token: CsrfToken,
}

// TODO: Completar con lo que dice la doc de Facebook
#[derive(Debug)]
pub struct Start {
    pub app_id: String,
    pub redirect_uri: RedirectUri,
    pub csrf_token: CsrfToken,
}

#[derive(Debug)]
pub struct TokenIssued {
    pub user_access_token: String,
    pub expires_in: Duration,
    pub token_type: String,
}

impl OAuth<TokenIssued> {
    pub async fn verify(
        self,
        client: &Client,
        app_access_token: &str,
        expected_app_id: &str,
    ) -> Result<OAuth<Authorized>> {
        let res = client
            .get("https://graph.facebook.com/v24.0/me")
            .query(&[
                ("access_token", &self.state.user_access_token),
                ("fields", &"id".to_string()),
            ])
            .send()
            .await?;

        let status = res.status();
        let body = res.text().await?;

        tracing::debug!(
            %status,
            %body,
            "Facebook /me verification response"
        );

        if !status.is_success() {
            return Err(AuthError::InvalidToken.into());
        }

        let me: serde_json::Value = serde_json::from_str(&body)?;

        let user_id = me
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or(AuthError::InvalidToken)?
            .to_string();

        Ok(OAuth {
            state: Authorized {
                user_access_token: self.state.user_access_token,
                app_id: expected_app_id.to_owned(),
                user_id: user_id,
                expires_at: Some(SystemTime::now() + self.state.expires_in),
                last_verified_at: SystemTime::now(),
            },
        })
    }
}

// TODO: Completar con lo que dice la doc de Facebook
#[derive(Debug)]
pub struct Authorized {
    pub user_access_token: String,
    pub app_id: String,
    pub user_id: String,
    pub expires_at: Option<SystemTime>,
    pub last_verified_at: SystemTime,
}

impl Authorized {
    pub fn locally_expired(&self) -> bool {
        match self.expires_at {
            Some(t) => SystemTime::now() >= t,
            None => false,
        }
    }

    pub fn needs_remote_verification(&self) -> bool {
        // TODO: Set correct expiration time
        let elapsed = match self.last_verified_at.elapsed() {
            Ok(d) => d,
            Err(_) => Duration::ZERO,
        };
        elapsed > Duration::from_hours(1)
    }

    pub async fn verify(&mut self, client: &Client, app_access_token: &str) -> Result<()> {
        let debug_endpoint = Url::parse_with_params(
            "https://graph.facebook.com/debug_token",
            &[
                ("input_token", self.user_access_token.clone()),
                ("access_token", app_access_token.to_string()),
            ],
        )
        .expect("Failed to parse user_access_token debug URL");

        let debug_data = {
            let res = client.get(debug_endpoint).send().await?;
            res.json::<DebugAccessTokenResponse>().await?
        };

        if !debug_data.data.is_valid {
            return Err(AuthError::InvalidToken.into());
        }

        if debug_data.data.app_id != self.app_id {
            return Err(AuthError::WrongApp.into());
        }

        self.last_verified_at = SystemTime::now();

        Ok(())
    }
}

#[derive(Debug)]
pub enum AuthState {
    LoggedIn(Authorized),
    LoggedOut,
}

impl AuthState {
    pub async fn ensure_valid(&mut self, client: &Client, app_access_token: &str) -> Result<()> {
        match self {
            AuthState::LoggedOut => Err(AuthError::NotLoggedIn.into()),
            AuthState::LoggedIn(auth) => {
                if auth.locally_expired() {
                    return Err(AuthError::Expired.into());
                }

                if auth.needs_remote_verification() {
                    auth.verify(client, app_access_token).await?;
                }

                Ok(())
            }
        }
    }
}

pub struct OAuth<S> {
    state: S,
}

impl OAuth<Start> {
    pub fn new(app_id: String, redirect_uri: RedirectUri) -> Self {
        let csrf_token = CsrfToken::generate();

        OAuth {
            state: Start {
                app_id,
                redirect_uri,
                csrf_token,
            },
        }
    }

    pub fn redirect(self, config_id: String) -> (Redirect, CsrfToken) {
        let url = Url::parse_with_params(
            "https://www.facebook.com/v24.0/dialog/oauth",
            &[
                ("client_id", &self.state.app_id),
                ("redirect_uri", &self.state.redirect_uri.to_string()),
                ("state", &self.state.csrf_token.to_string()),
                ("response_type", &"code".to_string()),
                ("config_id", &config_id),
            ],
        )
        .expect("Couldn't parse redirect url");

        (Redirect::temporary(url.as_str()), self.state.csrf_token)
    }

    pub fn from_callback(
        code: String,
        csrf_token: CsrfToken,
        redirect_uri: RedirectUri,
    ) -> OAuth<Redirected> {
        OAuth {
            state: Redirected {
                code,
                redirect_uri,
                csrf_token,
            },
        }
    }
}

impl OAuth<Redirected> {
    pub async fn exchange_token(
        self,
        client: &Client,
        app_id: &str,
        app_secret: &str,
    ) -> Result<OAuth<TokenIssued>> {
        let res = client
            .get("https://graph.facebook.com/v24.0/oauth/access_token")
            .query(&[
                ("client_id", app_id),
                ("client_secret", app_secret),
                ("redirect_uri", self.state.redirect_uri.as_str()),
                ("code", &self.state.code),
            ])
            .send()
            .await?
            .error_for_status()?;

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
            token_type: String,
            expires_in: u64,
        }

        let token = res.json::<TokenResponse>().await?;

        Ok(OAuth {
            state: TokenIssued {
                user_access_token: token.access_token,
                token_type: token.token_type,
                expires_in: Duration::from_secs(token.expires_in),
            },
        })
    }
}

#[axum::debug_handler]
#[instrument(skip(app, cookies))]
pub async fn fb_login(State(app): State<AppState>, cookies: CookieJar) -> axum::response::Response {
    if let Some(session_cookie) = cookies.get("session_id") {
        if let Ok(session_id) = SessionId::parse(session_cookie.value()) {
            if let Ok(Some(auth)) = load_session(&app.db, &session_id) {
                debug!("User already logged in");
                return (StatusCode::OK, auth.user_access_token).into_response();
            }
        }
    }

    let start_auth = OAuth::new(app.config.app_id, RedirectUri::default());
    let (redirect, csrf_token) = start_auth.redirect(app.config.fb_config_id);

    // TODO: Work on this later
    const IS_COOKIE_SECURE: bool = false;
    #[cfg(not(debug_assertions))]
    const _: () = assert!(IS_COOKIE_SECURE, "Cookies must be secure in release builds");

    let csrf_cookie = Cookie::build(("fb_oauth_csrf", csrf_token.to_string()))
        .http_only(true)
        .secure(IS_COOKIE_SECURE)
        .same_site(SameSite::Lax)
        .path("/")
        .build();

    let mut response = redirect.into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        csrf_cookie
            .to_string()
            .parse()
            .expect("Couldn't parse csrf_cookie"),
    );

    info!("CSRF cookie set");

    response
}

#[axum::debug_handler]
#[instrument("Facebook callback stub", skip(app, cookies, params))]
pub async fn fb_callback(
    State(app): State<AppState>,
    cookies: axum_extra::extract::CookieJar,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if let Some(error) = params.get("error") {
        let reason = params.get("error_reason");
        let desc = params.get("error_description");

        tracing::warn!(error, ?reason, ?desc, "Facebook OAuth error");

        return (
            StatusCode::UNAUTHORIZED,
            cookies,
            "Facebook login was cancelled or failed",
        );
    }

    let code = match params.get("code") {
        Some(c) => c.to_owned(),
        None => return (StatusCode::BAD_REQUEST, cookies, "Missing code"),
    };

    let returned_state = match params.get("state") {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, cookies, "Missing state"),
    };

    let stored_csrf = match cookies.get("fb_oauth_csrf") {
        Some(c) => c.value().to_owned(),
        None => return (StatusCode::BAD_REQUEST, cookies, "Missing CSRF cookie"),
    };

    if *returned_state != stored_csrf {
        return (StatusCode::UNAUTHORIZED, cookies, "Invalid CSRF");
    }

    tracing::debug!("CSRF validated");

    let redirected =
        OAuth::from_callback(code, CsrfToken::from(stored_csrf), app.config.redirect_uri);

    let token_issued = match redirected
        .exchange_token(&app.client, &app.config.app_id, &app.config.app_secret)
        .await
    {
        Ok(a) => a,
        Err(_) => return (StatusCode::BAD_GATEWAY, cookies, "Token exchange failed"),
    };

    tracing::debug!("Token exchanged");

    let auth_token = match token_issued
        .verify(
            &app.client,
            &app.config.app_access_token,
            &app.config.app_id,
        )
        .await
    {
        Ok(auth) => auth,
        Err(_) => {
            error!("Failed to verify issued token");
            return (StatusCode::UNAUTHORIZED, cookies, "Invalid token");
        }
    };

    tracing::debug!("Token verified");

    let session_id = SessionId::new();
    // TODO: Remove this connection creation
    let conn = Connection::open(app.db).expect("COULDNT OPEN DB");

    store_session(&conn, &session_id, &auth_token.state)
        .expect("Some error happened while storing the session id in the cookies");

    let cookies = cookies.add(build_session_cookie(session_id));

    tracing::info!(
        user_id = %auth_token.state.user_id,
        "login successful"
    );

    (StatusCode::ACCEPTED, cookies, "Authorized")
}

#[derive(Deserialize, Serialize, Debug)]
pub struct CsrfToken(String);

impl CsrfToken {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl From<String> for CsrfToken {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl Deref for CsrfToken {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The Redirect URL that facebook stores when trying to log in via OAuth and is called back
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RedirectUri(Url);

impl Default for RedirectUri {
    fn default() -> Self {
        let uri = Url::from_str("http://localhost:3000/facebook/oauth/callback")
            .expect("Couldn't parse URl from str");
        Self(uri)
    }
}

impl Deref for RedirectUri {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
