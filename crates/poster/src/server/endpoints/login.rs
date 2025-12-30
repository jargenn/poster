// Based on https://developers.facebook.com/docs/facebook-login/guides/advanced/manual-flow, (2025-12-26)

use std::collections::HashMap;

use crate::{
    cookies::{SessionId, build_session_cookie},
    db,
    server::AppState,
};
use axum::{
    Extension,
    extract::{Query, State},
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use facebook_graph_api::auth::{CsrfToken, OAuth};
use reqwest::{Client, StatusCode, header};
use secrecy::ExposeSecret;
use sqlx::PgPool;
use tracing::{debug, error, info, instrument};

#[axum::debug_handler]
#[instrument("OAuth login endpoint", skip(app, cookies, pool))]
pub async fn fb_login(
    State(app): State<AppState>,
    cookies: CookieJar,
    Extension(pool): Extension<PgPool>,
) -> axum::response::Response {
    if let Some(session_cookie) = cookies.get("session_id") {
        debug!(%session_cookie, "cookie jar has a session_id");
        if let Ok(session_id) = SessionId::parse(session_cookie.value()) {
            let mut conn = pool
                .acquire()
                .await
                .expect("Couldn't get access to a connection in the pool");

            if let Ok(Some(_)) = db::session::load_session(&mut conn, &session_id).await {
                info!("User is already logged in");
                return (StatusCode::CONFLICT, "User is already logged in").into_response();
            }
        }
    }

    info!("Starting user authentication");

    let start_auth = OAuth::new(app.config.app_id, app.config.redirect_uri);
    let (redirect_url, csrf_token) = start_auth.redirect(app.config.fb_config_id.expose_secret());
    let redirect = Redirect::temporary(redirect_url.as_str());

    let csrf_cookie = Cookie::build(("fb_oauth_csrf", csrf_token.to_string()))
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
#[instrument("Login Facebook callback", skip(app, cookies, params, pool, client))]
pub async fn fb_callback(
    State(app): State<AppState>,
    Extension(client): Extension<Client>,
    Extension(pool): Extension<PgPool>,
    cookies: axum_extra::extract::CookieJar,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let mut conn = pool
        .acquire()
        .await
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

    let Some(returned_state) = params.get("state") else {
        return (StatusCode::BAD_REQUEST, cookies, "Missing state");
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
        Err(e) => {
            error!(%e, "Failed to exchange token");
            return (StatusCode::BAD_GATEWAY, cookies, "Token exchange failed");
        }
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
        Err(e) => {
            error!(%e, "Failed to verify issued token");
            return (StatusCode::UNAUTHORIZED, cookies, "Invalid token");
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

    (StatusCode::ACCEPTED, cookies, "Authorized")
}
