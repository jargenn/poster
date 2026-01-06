// Based on https://deto_ownedvelopers.facebook.com/docs/facebook-login/guides/advanced/manual-flow, (2025-12-26)
use std::collections::HashMap;

use crate::{error::Error, server::PosterState, session_state::TypedSession, storage::db};
use axum::{
    Extension,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
};
use facebook_graph_api::auth::{CsrfToken, OAuth, RedirectUri};
use reqwest::{Client, StatusCode};
use tracing::{debug, error, info, instrument};

#[instrument("OAuth login endpoint", skip(state, session))]
pub async fn fb_login(
    Path(config_id): Path<String>,
    State(state): State<PosterState>,
    session: TypedSession,
) -> Response {
    let Some(_user_id) = session.get_user_id().await.ok().flatten() else {
        error!("No user_id in session - must call /login first");
        return (StatusCode::UNAUTHORIZED, "Must login first").into_response();
    };

    let Some(config) = state.fb_app_config.get(&config_id).await else {
        let msg = "There is no entry in the cache with that key";
        error!(key = config_id, "{msg}");

        return (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response();
    };

    info!("Starting user authentication");

    let redirect_url = format!("{}/{}", *config.redirect_url, config_id);
    debug!("Constructed redirect_url: {redirect_url}");

    let start_auth = OAuth::new(config.app_id, redirect_url);
    let (redirect_url, csrf_token) = start_auth.redirect(&config.app_config_id);
    let redirect = Redirect::temporary(redirect_url.as_str());

    session
        .insert_csrf(csrf_token)
        .await
        .expect("Should be able to insert `fb_oauth_csrf` in the session");

    info!("CSRF cookie set");

    redirect.into_response()
}

#[axum::debug_handler]
#[instrument("Login Facebook callback", skip(state, params, client, session))]
pub async fn fb_callback(
    Path(config_id): Path<String>,
    State(state): State<PosterState>,
    Extension(client): Extension<Client>,
    session: TypedSession,
    Query(params): Query<HashMap<String, String>>,
) -> Result<(StatusCode, String), Error> {
    tracing::debug!(?params, "Received callback params");
    let mut conn = state.pool.acquire().await.map_err(Error::Database)?;

    let Some(config) = state.fb_app_config.get(&config_id).await else {
        let msg = String::from("There is no entry in the cache with that key");
        error!(key = config_id, "{msg}");

        return Ok((StatusCode::INTERNAL_SERVER_ERROR, msg));
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

    // Make sure to reconstruct the same URL sent to the oauth dialog, where it had the UUID key
    // of the cache, take a look at how is this route defined before making changes to this.
    let redirect_uri =
        match RedirectUri::try_from(format!("{}/{}", config.redirect_url.to_string(), config_id)) {
            Err(err) => {
                error!("{}", err.to_string());
                return Ok((StatusCode::INTERNAL_SERVER_ERROR, "Invalid URI".to_owned()));
            }
            Ok(uri) => uri,
        };

    let redirected = OAuth::from_callback(code, CsrfToken::from(stored_csrf), redirect_uri);

    let token_issued = match redirected
        .exchange_token(&client, &config.app_id, &config.app_secret)
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
        .verify(&client, &config.app_id, &config.app_secret)
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
