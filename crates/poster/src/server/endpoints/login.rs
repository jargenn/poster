// Based on https://deto_ownedvelopers.facebook.com/docs/facebook-login/guides/advanced/manual-flow, (2025-12-26)
use std::collections::HashMap;

use crate::{
    cookies::{SessionId, build_session_cookie},
    db,
    error::Error,
    server::AppState,
};
use axum::{
    Extension,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use facebook_graph_api::auth::{CsrfToken, OAuth, RedirectUri};
use reqwest::{Client, StatusCode, header};
use tracing::{debug, error, info, instrument};

#[axum::debug_handler]
#[instrument("OAuth login endpoint", skip(state, cookies))]
pub async fn fb_login(
    Path(config_id): Path<String>,
    State(state): State<AppState>,
    cookies: CookieJar,
) -> Response {
    if let Some(session_cookie) = cookies.get("session_id") {
        debug!(%session_cookie, "cookie jar has a session_id");
        if let Ok(session_id) = SessionId::parse(session_cookie.value()) {
            let mut conn = state
                .pool
                .acquire()
                .await
                .expect("Couldn't get access to a connection in the pool");

            if let Ok(Some(_)) = db::session::load_session(&mut conn, &session_id).await {
                info!("User is already logged in");
                return (StatusCode::CONFLICT, "User is already logged in").into_response();
            }
        }
    }

    let Some(config) = state.user_config.get(&config_id).await else {
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

    let csrf_cookie = Cookie::build(("fb_oauth_csrf", csrf_token.to_owned()))
        .http_only(true)
        .secure({
            #[cfg(debug_assertions)]
            {
                false
            }
            #[cfg(not(debug_assertions))]
            {
                true
            }
        })
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
#[instrument("Login Facebook callback", skip(state, cookies, params, client))]
pub async fn fb_callback(
    Path(config_id): Path<String>,
    State(state): State<AppState>,
    Extension(client): Extension<Client>,
    cookies: CookieJar,
    Query(params): Query<HashMap<String, String>>,
) -> Result<(StatusCode, CookieJar, String), Error> {
    tracing::debug!(?params, "Received callback params");
    let mut conn = state.pool.acquire().await.map_err(Error::Database)?;

    let Some(config) = state.user_config.get(&config_id).await else {
        let msg = String::from("There is no entry in the cache with that key");
        error!(key = config_id, "{msg}");

        return Ok((StatusCode::INTERNAL_SERVER_ERROR, cookies, msg));
    };

    if let Some(error) = params.get("error") {
        let reason = params.get("error_reason");
        let desc = params.get("error_description");

        tracing::warn!(error, ?reason, ?desc, "Facebook OAuth error");

        return Ok((
            StatusCode::UNAUTHORIZED,
            cookies,
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
        None => return Ok((StatusCode::BAD_REQUEST, cookies, "Missing code".to_owned())),
    };

    let Some(returned_state) = params.get("state") else {
        return Ok((StatusCode::BAD_REQUEST, cookies, "Missing state".to_owned()));
    };

    let stored_csrf = match cookies.get("fb_oauth_csrf") {
        Some(c) => c.value().to_owned(),
        None => {
            return Ok((
                StatusCode::BAD_REQUEST,
                cookies,
                "Missing CSRF cookie".to_owned(),
            ));
        }
    };

    if *returned_state != stored_csrf {
        return Ok((StatusCode::UNAUTHORIZED, cookies, "Invalid CSRF".to_owned()));
    }

    tracing::debug!("CSRF validated");

    // Make sure to reconstruct the same URL sent to the oauth dialog, where it had the UUID key
    // of the cache, take a look at how is this route defined before making changes to this.
    let redirect_uri =
        match RedirectUri::try_from(format!("{}/{}", config.redirect_url.to_string(), config_id)) {
            Err(err) => {
                error!("{}", err.to_string());
                return Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    cookies,
                    "Invalid URI".to_owned(),
                ));
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
            return Ok((
                StatusCode::BAD_GATEWAY,
                cookies,
                "Token exchange failed".to_owned(),
            ));
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
            return Ok((
                StatusCode::UNAUTHORIZED,
                cookies,
                "Invalid token".to_owned(),
            ));
        }
    };

    tracing::debug!("Token verified");

    let session_id = SessionId::new();

    db::session::store_session(&mut conn, &session_id, &auth_token.state)
        .await
        .expect("Some error happened while storing the session id in the cookies");

    let cookies = cookies.add(build_session_cookie(&session_id));

    tracing::info!(
        user_id = %auth_token.state.user_id,
        "login successful"
    );

    Ok((StatusCode::ACCEPTED, cookies, "Authorized".to_owned()))
}
