use std::{
    ops::Deref,
    time::{Duration, SystemTime},
};

use axum::response::Redirect;
use eyre::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
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
    pub issued_at: SystemTime,
}

impl OAuth<TokenIssued> {
    pub async fn verify(self, client: &Client, expected_app_id: &str) -> Result<OAuth<Authorized>> {
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
                user_id,
                expires_at: Some(self.state.issued_at + self.state.expires_in),
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
    pub state: S,
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

    pub fn redirect(self, config_id: &str) -> (Redirect, CsrfToken) {
        let url = Url::parse_with_params(
            "https://www.facebook.com/v24.0/dialog/oauth",
            &[
                ("client_id", &self.state.app_id),
                ("redirect_uri", &self.state.redirect_uri.to_string()),
                ("state", &self.state.csrf_token.to_string()),
                ("response_type", &"code".to_string()),
                ("config_id", &config_id.to_string()),
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
                issued_at: SystemTime::now(),
            },
        })
    }
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

impl Deref for RedirectUri {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
