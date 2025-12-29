use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Debug,
    ops::Deref,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::debug;
use url::Url;

use crate::{AuthError, Error};

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
    issued_at: u64,
    #[serde(default)]
    metadata: Option<DebugResponseMetadata>,
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
    pub expires_in: Option<Duration>,
    pub token_type: String,
    pub issued_at: SystemTime,
}

impl OAuth<TokenIssued> {
    pub async fn verify(
        self,
        client: &Client,
        expected_app_id: &str,
        app_secret: &str,
    ) -> Result<OAuth<Authorized>, Error> {
        let app_access_token = format!("{}|{}", expected_app_id, app_secret);

        let debug_endpoint = Url::parse_with_params(
            "https://graph.facebook.com/v24.0/debug_token",
            &[
                ("input_token", &self.state.user_access_token),
                ("access_token", &app_access_token),
            ],
        )
        .expect("Failed to parse debug_token URL");

        let res = client.get(debug_endpoint).send().await?;
        let status = res.status();
        let body = res.text().await?;

        tracing::debug!(
            %status,
            %body,
            "Facebook debug_token verification response"
        );

        if !status.is_success() {
            return Err(AuthError::InvalidToken)?;
        }

        let debug_data: DebugAccessTokenResponse = serde_json::from_str(&body)?;

        if !debug_data.data.is_valid {
            return Err(AuthError::InvalidToken)?;
        }

        if debug_data.data.app_id != expected_app_id {
            return Err(AuthError::WrongApp)?;
        }

        // A expires_at with value 0 could mean that it is a long-lived token
        let expires_at = match debug_data.data.expires_at {
            Some(0) | None => None,
            Some(timestamp) => Some(UNIX_EPOCH + Duration::from_secs(timestamp)),
        };

        if let Some(exp) = expires_at {
            if SystemTime::now() >= exp {
                return Err(AuthError::Expired)?;
            }
        }

        Ok(OAuth {
            state: Authorized {
                user_access_token: self.state.user_access_token,
                app_id: expected_app_id.to_owned(),
                user_id: debug_data.data.user_id,
                expires_at,
                last_verified_at: SystemTime::now(),
            },
        })
    }
}

// TODO: Completar con lo que dice la doc de Facebook
#[derive(Debug, Clone)]
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

    pub async fn verify(
        &mut self,
        client: &Client,
        app_id: &str,
        app_secret: &str,
    ) -> Result<(), Error> {
        let app_access_token = format!("{}|{}", app_id, app_secret);

        let debug_endpoint = Url::parse_with_params(
            "https://graph.facebook.com/v24.0/debug_token",
            &[
                ("input_token", self.user_access_token.clone()),
                ("access_token", app_access_token),
            ],
        )
        .expect("Failed to parse user_access_token debug URL");

        let res = client.get(debug_endpoint).send().await?;
        let status = res.status();
        let body = res.text().await?;

        tracing::debug!(
            %status,
            %body,
            "Facebook debug_token verification response"
        );

        if !status.is_success() {
            return Err(AuthError::InvalidToken)?;
        }

        let debug_data: DebugAccessTokenResponse = serde_json::from_str(&body)?;

        if !debug_data.data.is_valid {
            return Err(AuthError::InvalidToken)?;
        }

        if debug_data.data.app_id != app_id {
            return Err(AuthError::WrongApp)?;
        }

        // A expires_at with value 0 could mean that it is a long-lived token
        let expires_at = match debug_data.data.expires_at {
            None | Some(0) => None,
            Some(t) => Some(UNIX_EPOCH + Duration::from_secs(t)),
        };

        if let Some(exp) = expires_at {
            if SystemTime::now() >= exp {
                return Err(AuthError::Expired)?;
            }
        }

        self.expires_at = expires_at;
        self.last_verified_at = SystemTime::now();

        Ok(())
    }
}

#[derive(Debug)]
pub enum AuthState {
    LoggedIn(Authorized),
    LoggedOut,
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

    /// Returns the redirect URI and a Cross-Site Reference Token
    pub fn redirect(self, config_id: &str) -> (Url, CsrfToken) {
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

        (url, self.state.csrf_token)
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
    ) -> Result<OAuth<TokenIssued>, Error> {
        let res = client
            .get("https://graph.facebook.com/v24.0/oauth/access_token")
            .query(&[
                ("client_id", app_id),
                ("client_secret", app_secret),
                ("redirect_uri", self.state.redirect_uri.as_str()),
                ("code", &self.state.code),
            ])
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to send OAuth token request");
                e
            })?;

        let status = res.status();
        let body = res.text().await?;

        debug!(
            status = %status,
            body = %body,
            "Facebook OAuth token response received"
        );

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
            token_type: String,
            expires_in: Option<u64>,
        }

        let token = serde_json::from_str::<TokenResponse>(&body)?;

        Ok(OAuth {
            state: TokenIssued {
                user_access_token: token.access_token,
                token_type: token.token_type,
                expires_in: token.expires_in.map(Duration::from_secs),
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
#[derive(Serialize, Deserialize, Clone)]
pub struct RedirectUri(Url);

impl Debug for RedirectUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RedirectUri")
            .field(&self.0.as_str())
            .finish()
    }
}

impl Deref for RedirectUri {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
