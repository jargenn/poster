use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Debug,
    ops::Deref,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::debug;
use url::{ParseError, Url};

use thiserror::Error as ThisError;

pub trait OAuthProvider {
    fn authorization_url(&self) -> &'static str;
    fn token_url(&self) -> &'static str;
    fn authorization_parameters(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    async fn verify_access_token(
        &self,
        client: &Client,
        access_token: &str,
        app_id: &str,
        app_secret: &str,
    ) -> Result<VerifiedToken, OAuthError>;
}

#[derive(Debug, Clone)]
pub struct FacebookProvider {
    pub config_id: String,
}

impl OAuthProvider for FacebookProvider {
    fn authorization_url(&self) -> &'static str {
        "https://www.facebook.com/v24.0/dialog/oauth"
    }

    fn token_url(&self) -> &'static str {
        "https://graph.facebook.com/v24.0/oauth/access_token"
    }

    fn authorization_parameters(&self) -> Vec<(String, String)> {
        vec![("config_id".to_owned(), self.config_id.clone())]
    }

    async fn verify_access_token(
        &self,
        client: &Client,
        access_token: &str,
        app_id: &str,
        app_secret: &str,
    ) -> Result<VerifiedToken, OAuthError> {
        let endpoint = Url::parse_with_params(
            "https://graph.facebook.com/v24.0/debug_token",
            &[
                ("input_token", access_token),
                ("access_token", &format!("{app_id}|{app_secret}")),
            ],
        )
        .expect("valid Facebook debug-token URL");
        let response = client.get(endpoint).send().await?;
        let status = response.status();
        let body = response.text().await?;
        tracing::debug!(%status, %body, "Facebook debug_token verification response");
        if !status.is_success() {
            return Err(AuthError::InvalidToken.into());
        }
        let data: DebugAccessTokenResponse = serde_json::from_str(&body)?;
        if !data.data.is_valid {
            return Err(AuthError::InvalidToken.into());
        }
        if data.data.app_id != app_id {
            return Err(AuthError::WrongApp.into());
        }
        let expires_at = match data.data.expires_at {
            None | Some(0) => None,
            Some(timestamp) => Some(UNIX_EPOCH + Duration::from_secs(timestamp)),
        };
        if expires_at.is_some_and(|expiration| SystemTime::now() >= expiration) {
            return Err(AuthError::Expired.into());
        }
        Ok(VerifiedToken {
            app_id: app_id.to_owned(),
            user_id: data.data.user_id,
            expires_at,
        })
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedToken {
    pub app_id: String,
    pub user_id: String,
    pub expires_at: Option<SystemTime>,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("The access token you are using doesn't belong to the this App")]
    WrongApp,
    #[error("Provided access token is no longer valid")]
    InvalidToken,
    #[error("You are not logged in to the App")]
    NotLoggedIn,
    #[error("Access token has expired")]
    Expired,
}

#[derive(Debug, ThisError)]
pub enum OAuthError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("Network error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("OAuth provider error: {0}")]
    Provider(String),
}

// /// TODO: Search for what scopes are there.
// #[derive(Debug, Serialize, Deserialize)]
// #[serde(rename_all = "snake_case")] enum Scope { Email, PublishActions,
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
    pub redirect_uri: String,
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
        provider: &impl OAuthProvider,
        client: &Client,
        expected_app_id: &str,
        app_secret: &str,
    ) -> Result<OAuth<Authorized>, OAuthError> {
        let verified = provider
            .verify_access_token(
                client,
                &self.state.user_access_token,
                expected_app_id,
                app_secret,
            )
            .await?;

        Ok(OAuth {
            state: Authorized {
                user_access_token: self.state.user_access_token,
                app_id: verified.app_id,
                user_id: verified.user_id,
                expires_at: verified.expires_at,
                last_verified_at: SystemTime::now(),
            },
        })
    }
}

/// Carries the State of an OAuth Authorized User
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
        elapsed > Duration::from_secs(60 * 60)
    }

    pub async fn verify(
        &mut self,
        provider: &impl OAuthProvider,
        client: &Client,
        app_id: &str,
        app_secret: &str,
    ) -> Result<(), OAuthError> {
        let verified = provider
            .verify_access_token(client, &self.user_access_token, app_id, app_secret)
            .await?;
        self.expires_at = verified.expires_at;
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
    pub fn new(app_id: String, redirect_uri: String) -> Self {
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
    pub fn redirect(self, provider: &impl OAuthProvider) -> (Url, CsrfToken) {
        let redirect_url = &self.state.redirect_uri.clone();
        debug!(
            redirect_url,
            "This is the redirect_uri sent to the oauth dialog"
        );
        let client = oauth2::basic::BasicClient::new(oauth2::ClientId::new(self.state.app_id))
            .set_auth_uri(
                oauth2::AuthUrl::new(provider.authorization_url().to_owned())
                    .expect("valid Facebook authorization URL"),
            )
            .set_redirect_uri(
                oauth2::RedirectUrl::new(redirect_url.to_owned()).expect("valid redirect URL"),
            );
        let request = provider.authorization_parameters().into_iter().fold(
            client.authorize_url(|| oauth2::CsrfToken::new(self.state.csrf_token.to_string())),
            |request, (key, value)| request.add_extra_param(key, value),
        );
        let (url, _) = request.url();
        let url = Url::parse(url.as_str()).expect("oauth2 generated a valid authorization URL");

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
        provider: &impl OAuthProvider,
        client: &Client,
        app_id: &str,
        app_secret: &str,
    ) -> Result<OAuth<TokenIssued>, OAuthError> {
        let redirect_url = self.state.redirect_uri.to_string();
        debug!(
            redirect_url,
            "This is the redirect_uri sent during the exchange of CSRF tokens"
        );
        let oauth_client =
            oauth2::basic::BasicClient::new(oauth2::ClientId::new(app_id.to_owned()))
                .set_client_secret(oauth2::ClientSecret::new(app_secret.to_owned()))
                .set_auth_uri(
                    oauth2::AuthUrl::new(provider.authorization_url().to_owned())
                        .expect("valid Facebook authorization URL"),
                )
                .set_token_uri(
                    oauth2::TokenUrl::new(provider.token_url().to_owned())
                        .expect("valid Facebook token URL"),
                )
                .set_redirect_uri(
                    oauth2::RedirectUrl::new(redirect_url).expect("valid redirect URL"),
                );
        use oauth2::TokenResponse;
        let token = oauth_client
            .exchange_code(oauth2::AuthorizationCode::new(self.state.code))
            .request_async(client)
            .await
            .map_err(|error| OAuthError::Provider(error.to_string()))?;

        Ok(OAuth {
            state: TokenIssued {
                user_access_token: token.access_token().secret().to_owned(),
                token_type: token.token_type().as_ref().to_owned(),
                expires_in: token.expires_in(),
                issued_at: SystemTime::now(),
            },
        })
    }
}
#[derive(Deserialize, Serialize, Debug)]
pub struct CsrfToken(String);

impl CsrfToken {
    pub fn generate() -> Self {
        Self(oauth2::CsrfToken::new_random().secret().to_owned())
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

impl TryFrom<String> for RedirectUri {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self(Url::parse(&value)?))
    }
}

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
