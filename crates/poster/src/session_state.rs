use axum::extract::FromRequestParts;
use facebook_graph_api::auth::CsrfToken;
use tower_sessions::{Session, session::Error};
use tracing::instrument;
use uuid::Uuid;

pub struct TypedSession(Session);

impl TypedSession {
    const USER_ID_KEY: &'static str = "user_id";
    const FB_OAUTH_CSRF_KEY: &'static str = "fb_oauth_csrf";

    pub async fn renew(&self) -> Result<(), Error> {
        self.0.cycle_id().await
    }

    pub async fn insert_user_id(&self, user_id: Uuid) -> Result<(), Error> {
        self.0.insert(Self::USER_ID_KEY, user_id).await
    }
    pub async fn get_user_id(&self) -> Result<Option<Uuid>, Error> {
        self.0.get(Self::USER_ID_KEY).await
    }

    pub async fn insert_csrf(&self, token: CsrfToken) -> Result<(), Error> {
        self.0.insert(Self::FB_OAUTH_CSRF_KEY, token).await
    }

    pub async fn get_csrf(&self) -> Result<Option<CsrfToken>, Error> {
        self.0.get(Self::FB_OAUTH_CSRF_KEY).await
    }
}

impl<S> FromRequestParts<S> for TypedSession
where
    S: Send + Sync,
    Session: FromRequestParts<S>,
{
    type Rejection = <Session as FromRequestParts<S>>::Rejection;

    #[instrument("building typed_session", skip(parts, state))]
    async fn from_request_parts(
        parts: &mut http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state).await?;
        Ok(TypedSession(session))
    }
}
