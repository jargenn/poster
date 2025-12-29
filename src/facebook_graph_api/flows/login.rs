// Based on https://developers.facebook.com/docs/facebook-login/guides/advanced/manual-flow, (2025-12-26)

use std::collections::HashMap;

use crate::{
    cookies::{SessionId, build_session_cookie},
    db::{load_session, store_session},
    facebook_graph_api::auth::{CsrfToken, OAuth},
    server::AppState,
};
use axum::{
    Extension,
    extract::{Query, State},
    response::IntoResponse,
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use reqwest::{Client, StatusCode, header};
use secrecy::ExposeSecret;
use tracing::{debug, error, info, instrument};

#[axum::debug_handler]
#[instrument("Starting login flow", skip(app, cookies, pool))]
pub async fn fb_login(
    State(app): State<AppState>,
    cookies: CookieJar,
    Extension(pool): Extension<Pool<SqliteConnectionManager>>,
) -> axum::response::Response {
    if let Some(session_cookie) = cookies.get("session_id") {
        debug!(%session_cookie, "cookie jar has a session_id");
        if let Ok(session_id) = SessionId::parse(session_cookie.value()) {
            let conn = pool
                .get()
                .expect("Couldn't get access to a connection in the pool");

            if let Ok(Some(_)) = load_session(&conn, &session_id) {
                info!("User is already logged in");
                return (StatusCode::CONFLICT, "User is already logged in").into_response();
            }
        }
    }

    let start_auth = OAuth::new(app.config.app_id, app.config.redirect_uri);
    let (redirect, csrf_token) = start_auth.redirect(app.config.fb_config_id.expose_secret());

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
#[instrument("Facebook callback stub", skip(app, cookies, params, pool))]
pub async fn fb_callback(
    State(app): State<AppState>,
    Extension(client): Extension<Client>,
    Extension(pool): Extension<Pool<SqliteConnectionManager>>,
    cookies: axum_extra::extract::CookieJar,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let conn = pool
        .get()
        .expect("Couldn't get access to a connection in the pool");

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
        .exchange_token(
            &client,
            &app.config.app_id,
            app.config.app_secret.expose_secret(),
        )
        .await
    {
        Ok(a) => a,
        Err(_) => return (StatusCode::BAD_GATEWAY, cookies, "Token exchange failed"),
    };

    tracing::debug!("Token exchanged");

    let auth_token = match token_issued
        .verify(
            &client,
            &app.config.app_id,
            app.config.app_secret.expose_secret(),
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

    store_session(&conn, &session_id, &auth_token.state)
        .expect("Some error happened while storing the session id in the cookies");

    let cookies = cookies.add(build_session_cookie(session_id));

    tracing::info!(
        user_id = %auth_token.state.user_id,
        "login successful"
    );

    (StatusCode::ACCEPTED, cookies, "Authorized")
}
