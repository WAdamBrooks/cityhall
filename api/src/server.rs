use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, patch, post, put};
use axum::Router;
use sea_orm::DatabaseConnection;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::error::AppError;
use crate::handlers::{
    agent_credentials, auth, dashboard, oidc, roles, settings, setup, signup, townhall,
    townhall_permanent, townhall_requests, users, workspace_config, workspaces,
};
use crate::proxy;
use crate::state::AppState;

/// Directory holding the built frontend. Overridable via `STATIC_DIR` for
/// container images that place the bundle elsewhere.
fn static_dir() -> PathBuf {
    std::env::var("STATIC_DIR")
        .unwrap_or_else(|_| "web/dist".to_string())
        .into()
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/change-password", post(auth::change_password))
        .route("/me/onboarding-dismissed", post(auth::dismiss_onboarding))
        .route("/auth/forgot-password", post(auth::forgot_password))
        .route("/auth/reset-password", post(auth::reset_password))
        .route("/auth/providers", get(oidc::providers))
        .route("/auth/oidc/login", get(oidc::login))
        .route("/auth/oidc/callback", get(oidc::callback))
        .route("/auth/register", post(signup::register))
        .route("/auth/verify-email", post(signup::verify_email))
        .route("/users", get(users::list).post(users::create))
        .route(
            "/users/{id}",
            get(users::get).patch(users::update).delete(users::delete),
        )
        .route("/dashboard", get(dashboard::overview))
        .route(
            "/me/dashboard-layout",
            get(dashboard::get_layout).put(dashboard::put_layout),
        )
        .route("/roles", get(roles::list).post(roles::create))
        .route("/roles/{id}", patch(roles::update).delete(roles::delete))
        .route("/permissions", get(roles::permissions))
        .route(
            "/workspaces",
            get(workspaces::list).patch(workspaces::bulk_set_version),
        )
        .route("/workspaces/me", get(workspaces::me))
        .route("/workspaces/me/restart", post(workspaces::restart_mine))
        .route("/workspaces/versions", get(workspaces::versions))
        .route(
            "/workspaces/{user_id}",
            patch(workspaces::set_version).delete(workspaces::destroy),
        )
        .route("/workspaces/{user_id}/start", post(workspaces::start))
        .route(
            "/workspaces/{user_id}/access-url",
            post(workspaces::access_url),
        )
        .route("/workspaces/{user_id}/stop", post(workspaces::stop))
        .route("/settings/smtp", get(settings::get).put(settings::update))
        .route("/settings/smtp/test", post(settings::test))
        .route(
            "/settings/oidc",
            get(oidc::get_settings).put(oidc::update_settings),
        )
        .route(
            "/settings/signup",
            get(signup::get_settings).put(signup::update_settings),
        )
        .route(
            "/settings/workspaces",
            get(workspaces::get_settings).put(workspaces::update_settings),
        )
        .route(
            "/settings/workspace-config",
            get(workspace_config::get_config).put(workspace_config::update_config),
        )
        .route("/settings/setup", get(setup::get).put(setup::update))
        .route(
            "/me/git-credential",
            get(workspace_config::get_git_credential)
                .put(workspace_config::update_git_credential)
                .delete(workspace_config::delete_git_credential),
        )
        .route(
            "/me/git-ssh-key",
            get(workspace_config::get_git_ssh_key)
                .put(workspace_config::update_git_ssh_key)
                .delete(workspace_config::delete_git_ssh_key),
        )
        .route("/me/agent-credentials", get(agent_credentials::get_mine))
        .route(
            "/me/agent-credentials/{env_var}",
            put(agent_credentials::put_mine).delete(agent_credentials::delete_mine),
        )
        .route(
            "/users/{user_id}/agent-credentials",
            get(agent_credentials::get_for_user),
        )
        .route(
            "/users/{user_id}/agent-credentials/{env_var}",
            put(agent_credentials::put_for_user).delete(agent_credentials::delete_for_user),
        )
        .route("/townhall/sessions", get(townhall::my_sessions))
        .route(
            "/townhall/agents",
            get(townhall::my_agents).post(townhall::register_agent),
        )
        .route(
            "/townhall/meetings",
            get(townhall::list_meetings).post(townhall::create_meeting),
        )
        .route(
            "/townhall/meetings/{id}",
            get(townhall::get_meeting).patch(townhall::update_meeting),
        )
        .route(
            "/townhall/meetings/{id}/agents",
            post(townhall::enroll_agent),
        )
        .route(
            "/townhall/meetings/{id}/requests",
            get(townhall_requests::list_requests).post(townhall_requests::ask_to_join),
        )
        .route(
            "/townhall/meetings/{id}/requests/{rid}",
            post(townhall_requests::decide_request).delete(townhall_requests::remove_request),
        )
        .route(
            "/townhall/meetings/{id}/requests/{rid}/respond",
            post(townhall_requests::respond_to_invite),
        )
        .route(
            "/townhall/meetings/{id}/invites",
            post(townhall_requests::invite),
        )
        .route("/townhall/directory", get(townhall_requests::directory))
        .route(
            "/townhall/meetings/{id}/participants/{pid}/permanent",
            put(townhall_permanent::make_permanent).delete(townhall_permanent::end_permanence),
        )
        .route(
            "/townhall/meetings/{id}/participants/{pid}/assistant",
            put(townhall::set_assistant),
        )
        .route(
            "/townhall/meetings/{id}/participants/{pid}/visibility",
            put(townhall::set_visibility),
        )
        .route(
            "/townhall/meetings/{id}/participants/{pid}/converse",
            put(townhall_permanent::set_converse),
        )
        .route(
            "/townhall/meetings/{id}/my-requests",
            get(townhall_requests::my_requests),
        )
        .route(
            "/townhall/meetings/{id}/participants/{pid}",
            patch(townhall::set_turn_mode).delete(townhall::dismiss_participant),
        )
        .route(
            "/townhall/meetings/{id}/messages",
            get(townhall::list_messages).post(townhall::post_human_message),
        )
        .route(
            "/townhall/meetings/{id}/call",
            post(townhall::call_on_agent),
        )
        // Fetched by a workspace container, authenticated by its own bearer
        // token rather than a session cookie.
        .route("/workspace-bundle", get(workspace_config::serve_bundle))
        // Unknown /api/* paths return a JSON 404 instead of falling through to
        // the SPA index (which the outer fallback_service would otherwise serve).
        .fallback(|| async { AppError::NotFound("not found") })
        .with_state(state)
}

pub fn router(state: AppState) -> Router {
    let dir = static_dir();
    // Serve the SPA: static assets first, fall back to index.html so client
    // routes (e.g. /login) resolve on hard refresh.
    let index = dir.join("index.html");
    // `.fallback` (not `.not_found_service`, which forces a 404 status) so
    // client routes like /login resolve with 200 on hard refresh.
    let spa = ServeDir::new(&dir).fallback(ServeFile::new(index));

    Router::new()
        .nest("/api", api_router(state))
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http())
}

pub fn build_state(db: DatabaseConnection) -> Result<AppState, Box<dyn std::error::Error>> {
    // HTTP/1.1 only and no redirect following: WebSocket upgrades need 1.1,
    // and a proxy passes redirects through rather than chasing them.
    let proxy_client = reqwest::Client::builder()
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let (orchestrator, provisioning) = crate::orchestrator::from_env()?;
    Ok(AppState {
        db,
        orchestrator,
        activity: Arc::default(),
        locks: Arc::default(),
        endpoints: Arc::default(),
        provisioning,
        versions: Arc::default(),
        metrics: Arc::default(),
        proxy_client,
    })
}

pub async fn serve(db: DatabaseConnection) -> Result<(), Box<dyn std::error::Error>> {
    let state = build_state(db)?;
    crate::workspaces::seed_default_version(&state.db).await?;
    tokio::spawn(crate::workspaces::idle_sweeper(state.clone()));
    tokio::spawn(crate::metrics::sampler(state.clone()));
    tokio::spawn(crate::townhall_runtime::permanent_ticker(state.clone()));

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("cityhall listening on http://{addr}");

    let proxy_addr = proxy::bind_addr();
    let proxy_listener = tokio::net::TcpListener::bind(&proxy_addr).await?;
    tracing::info!("workspace proxy listening on http://{proxy_addr}");

    let main_srv = axum::serve(listener, router(state.clone()));
    // ConnectInfo gives the proxy the peer IP for X-Forwarded-For.
    let proxy_srv = axum::serve(
        proxy_listener,
        proxy::router(state).into_make_service_with_connect_info::<SocketAddr>(),
    );
    tokio::try_join!(main_srv, proxy_srv)?;
    Ok(())
}
