// Based on https://deto_ownedvelopers.facebook.com/docs/facebook-login/guides/advanced/manual-flow, (2025-12-26)
use std::collections::HashMap;

use crate::{
    configuration::FbAppData, error::Error, server::PosterState, session_state::TypedSession,
    storage::db,
};
use axum::{
    Extension,
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
};
use auth::{CsrfToken, FacebookProvider, OAuth, RedirectUri};
use reqwest::{Client, StatusCode};
use tracing::{debug, error, info, instrument, warn};

#[instrument("OAuth login endpoint", skip(state, session))]
pub async fn fb_login(
    State(state): State<PosterState>,
    session: TypedSession,
) -> Result<Response, Error> {
    let Some(user_id) = session.get_user_id().await.ok().flatten() else {
        error!("No user_id in session - must call /login first");
        return Ok((StatusCode::UNAUTHORIZED, "Must login first").into_response());
    };

    let config = if let Some(config) = state.fb_app_config.get(&user_id.to_string()).await {
        config
    } else {
        warn!("There is no entry in the cache with that key");

        let mut conn = state.pool.acquire().await.map_err(Error::Database)?;
        let row = sqlx::query!(
            r#"
        select
            app_id,
            app_secret,
            app_config_id,
            redirect_url
        from user_configs where id = $1
    "#,
            user_id
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(Error::Database)?;

        let data = FbAppData {
            app_id: row.app_id,
            app_secret: row.app_secret,
            app_config_id: row.app_config_id,
            redirect_url: RedirectUri::try_from(row.redirect_url)
                .expect("Should be able to build a RedirectUri"),
        };

        state
            .fb_app_config
            .insert(user_id.to_string(), data.clone())
            .await;
        data
    };

    info!("Starting user authentication");
    let redirect_url = config.redirect_url.to_string();
    debug!("Constructed redirect_url: {redirect_url}");

    let start_auth = OAuth::new(config.app_id, redirect_url);

    error!(%config.app_config_id,"App config id");

    let provider = FacebookProvider {
        config_id: config.app_config_id.clone(),
    };
    let (redirect_url, csrf_token) = start_auth.redirect(&provider);
    let redirect = Redirect::temporary(redirect_url.as_str());

    session
        .insert_csrf(csrf_token)
        .await
        .expect("Should be able to insert `fb_oauth_csrf` in the session");

    info!("CSRF cookie set");

    Ok(redirect.into_response())
}

#[axum::debug_handler]
#[instrument("Login Facebook callback", skip(state, params, client, session))]
pub async fn fb_callback(
    State(state): State<PosterState>,
    Extension(client): Extension<Client>,
    session: TypedSession,
    Query(params): Query<HashMap<String, String>>,
) -> Result<(StatusCode, String), Error> {
    tracing::debug!(?params, "Received callback params");
    let mut conn = state.pool.acquire().await.map_err(Error::Database)?;

    let Some(user_id) = session.get_user_id().await.ok().flatten() else {
        error!("No user_id in session - must call /login first");
        return Ok((StatusCode::UNAUTHORIZED, "Must login first".to_string()));
    };

    let config = if let Some(config) = state.fb_app_config.get(&user_id.to_string()).await {
        config
    } else {
        warn!("There is no entry in the cache with that key");

        let row = sqlx::query!(
            r#"
        select
            app_id,
            app_secret,
            app_config_id,
            redirect_url
        from user_configs where id = $1
    "#,
            user_id
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(Error::Database)?;

        let data = FbAppData {
            app_id: row.app_id,
            app_secret: row.app_secret,
            app_config_id: row.app_config_id,
            redirect_url: RedirectUri::try_from(row.redirect_url)
                .expect("Should be able to build a RedirectUri"),
        };

        state
            .fb_app_config
            .insert(user_id.to_string(), data.clone())
            .await;
        data
    };

    if let Some(error) = params.get("error") {
        let reason = params.get("error_reason");
        let desc = params.get("error_description");

        tracing::warn!(error, ?reason, ?desc, "Facebook OAuth error");

        return Ok((
            StatusCode::UNAUTHORIZED,
            "Facebook login was cancelled or failed".to_owned(),
        ));
    }

    let code = match params.get("code") {
        Some(c) => {
            tracing::debug!(
                code_length = c.len(),
                code_value = c,
                "Extracted code from query params"
            );
            c.to_owned()
        }
        None => return Ok((StatusCode::BAD_REQUEST, "Missing code".to_owned())),
    };

    let Some(returned_state) = params.get("state") else {
        return Ok((StatusCode::BAD_REQUEST, "Missing state".to_owned()));
    };

    let stored_csrf = match session
        .get_csrf()
        .await
        .expect("Failed to find `fb_oauth_csrf` in the session")
    {
        Some(c) => c.to_owned(),
        None => {
            return Ok((StatusCode::BAD_REQUEST, "Missing CSRF cookie".to_owned()));
        }
    };

    if *returned_state != stored_csrf {
        return Ok((StatusCode::UNAUTHORIZED, "Invalid CSRF".to_owned()));
    }

    tracing::debug!("CSRF validated");

    let redirect_uri = match RedirectUri::try_from(config.redirect_url.to_string()) {
        Err(err) => {
            error!("{}", err.to_string());
            return Ok((StatusCode::INTERNAL_SERVER_ERROR, "Invalid URI".to_owned()));
        }
        Ok(uri) => uri,
    };

    let redirected = OAuth::from_callback(code, CsrfToken::from(stored_csrf), redirect_uri);
    let provider = FacebookProvider {
        config_id: config.app_config_id.clone(),
    };

    error!(secret=%config.app_secret, "App secret used");
    let token_issued = match redirected
        .exchange_token(&provider, &client, &config.app_id, &config.app_secret)
        .await
    {
        Ok(a) => a,
        Err(e) => {
            error!(%e, "Failed to exchange token");
            return Ok((StatusCode::BAD_GATEWAY, "Token exchange failed".to_owned()));
        }
    };

    tracing::debug!("Token exchanged");

    let auth_token = match token_issued
        .verify(&provider, &client, &config.app_id, &config.app_secret)
        .await
    {
        Ok(auth) => auth,
        Err(e) => {
            error!(%e, "Failed to verify issued token");
            return Ok((StatusCode::UNAUTHORIZED, "Invalid token".to_owned()));
        }
    };

    tracing::debug!("Token verified");

    let user_id = session
        .get_user_id()
        .await
        .expect("Expected to see some value in the session")
        .expect("The value in the session for `user_id` was None");

    db::session::store_oauth_data(&mut conn, &user_id, &auth_token.state)
        .await
        .expect("Some error happened while storing the session id in the cookies");

    tracing::info!(
        user_id = %auth_token.state.user_id,
        "login successful"
    );

    Ok((StatusCode::ACCEPTED, "Authorized".to_owned()))
}
