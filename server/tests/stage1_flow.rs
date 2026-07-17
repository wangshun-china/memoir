//! Integration tests for Stage-1 handlers.
//!
//! Uses database `memoir_test` by default so cargo test never pollutes the live
//! `memoir` application database. Set TEST_DATABASE_URL to override.
//! Requires Postgres + ALLOW_DEV_LOGIN for user-facing API tests only.

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use memoir_server::memoirs::DEFAULT_CHAPTER_TITLES;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

fn database_configured() -> bool {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
        .filter(|s| !s.is_empty())
        .is_some()
}

/// Point app_from_env at the isolated test DB (not the live memoir DB).
fn prepare_test_env() {
    dotenvy::dotenv().ok();
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://memoir:memoir@127.0.0.1:5433/memoir".into());
        // Prefer dedicated test database name.
        if base.ends_with("/memoir") {
            format!("{base}_test")
        } else if base.contains("/memoir?") {
            base.replacen("/memoir?", "/memoir_test?", 1)
        } else {
            "postgres://memoir:memoir@127.0.0.1:5433/memoir_test".into()
        }
    });
    std::env::set_var("DATABASE_URL", url);
    std::env::set_var("ALLOW_DEV_LOGIN", "1");
}

async fn cleanup_user(pool: &sqlx::PgPool, user_id: Uuid) {
    // Cascade removes memoirs / chapters / sessions / messages for this user.
    let _ = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await;
}

async fn body_json(body: Body) -> Value {
    let bytes: Bytes = body.collect().await.expect("body").to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
}

async fn json_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {t}"));
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let req = builder
        .body(match body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        })
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("response");
    let status = response.status();
    let json = if status == StatusCode::NO_CONTENT {
        Value::Null
    } else {
        body_json(response.into_body()).await
    };
    (status, json)
}

#[tokio::test]
async fn health_endpoint_ok() {
    if !database_configured() {
        eprintln!("skip: DATABASE_URL/TEST_DATABASE_URL not set");
        return;
    }
    prepare_test_env();
    let (app, _) = memoir_server::app_from_env().await.expect("app");
    let (status, json) = json_request(&app, "GET", "/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "memoir-server");
}

#[tokio::test]
async fn create_memoir_seeds_nine_default_chapters() {
    if !database_configured() {
        eprintln!("skip: DATABASE_URL/TEST_DATABASE_URL not set");
        return;
    }
    prepare_test_env();
    let (app, state) = memoir_server::app_from_env().await.expect("app");
    let openid = format!("test-memoir-{}", Uuid::new_v4());

    let (status, auth) = json_request(
        &app,
        "POST",
        "/api/v1/auth/dev-login",
        None,
        Some(json!({ "nickname": "test_creator", "openid": openid })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "auth: {auth}");
    let token = auth["token"].as_str().expect("token");
    let user_id = Uuid::parse_str(auth["user_id"].as_str().unwrap()).unwrap();

    let (status, memoir) = json_request(
        &app,
        "POST",
        "/api/v1/memoirs",
        Some(token),
        Some(json!({
            "subject_name": "TestSubjectA",
            "birth_year": 1948,
            "birth_place": "TestCity",
            "preferred_name": "SubjectA",
            "creator_relation": "child"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create memoir: {memoir}");
    assert_eq!(memoir["subject_name"], "TestSubjectA");
    assert!(memoir["title"].as_str().unwrap().contains("TestSubjectA"));

    let chapters = memoir["chapters"].as_array().expect("chapters array");
    assert_eq!(
        chapters.len(),
        DEFAULT_CHAPTER_TITLES.len(),
        "expected nine default chapters"
    );
    let titles: Vec<&str> = chapters
        .iter()
        .map(|c| c["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, DEFAULT_CHAPTER_TITLES.to_vec());

    let memoir_id = memoir["id"].as_str().unwrap();
    let (status, listed) = json_request(
        &app,
        "GET",
        &format!("/api/v1/memoirs/{memoir_id}/chapters"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list chapters: {listed}");
    let listed_arr = listed.as_array().unwrap();
    assert_eq!(listed_arr.len(), 9);
    assert_eq!(listed_arr[0]["title"], "童年与家庭");
    assert_eq!(listed_arr[8]["title"], "我想留下的话");

    cleanup_user(&state.pool, user_id).await;
}

#[tokio::test]
async fn interview_message_round_trip_and_finish() {
    if !database_configured() {
        eprintln!("skip: DATABASE_URL/TEST_DATABASE_URL not set");
        return;
    }
    prepare_test_env();
    let (app, state) = memoir_server::app_from_env().await.expect("app");
    let openid = format!("test-iv-{}", Uuid::new_v4());

    let (status, auth) = json_request(
        &app,
        "POST",
        "/api/v1/auth/dev-login",
        None,
        Some(json!({ "nickname": "test_interviewer", "openid": openid })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = auth["token"].as_str().unwrap();
    let user_id = Uuid::parse_str(auth["user_id"].as_str().unwrap()).unwrap();

    let (status, memoir) = json_request(
        &app,
        "POST",
        "/api/v1/memoirs",
        Some(token),
        Some(json!({ "subject_name": "TestSubjectB" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{memoir}");
    let memoir_id = memoir["id"].as_str().unwrap();

    let (status, session_body) = json_request(
        &app,
        "POST",
        &format!("/api/v1/memoirs/{memoir_id}/interviews"),
        Some(token),
        Some(json!({ "topic": "童年与家庭" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{session_body}");
    let session_id = session_body["session"]["id"].as_str().unwrap();
    assert_eq!(session_body["session"]["status"], "active");
    assert_eq!(session_body["opening_message"]["role"], "assistant");

    let user_text = "I walked to school when I was a child.";
    let (status, msg_resp) = json_request(
        &app,
        "POST",
        &format!("/api/v1/interviews/{session_id}/messages"),
        Some(token),
        Some(json!({ "content": user_text, "action": "normal" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{msg_resp}");
    assert_eq!(msg_resp["user_message"]["role"], "user");
    assert_eq!(msg_resp["user_message"]["content"], user_text);
    assert_eq!(msg_resp["assistant_message"]["role"], "assistant");
    let assistant = msg_resp["assistant_message"]["content"]
        .as_str()
        .unwrap_or("");
    assert!(!assistant.is_empty(), "assistant reply must be non-empty");

    let (status, messages) = json_request(
        &app,
        "GET",
        &format!("/api/v1/interviews/{session_id}/messages"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{messages}");
    let msgs = messages.as_array().unwrap();
    assert!(msgs.len() >= 3, "expected >=3 messages, got {}", msgs.len());
    let roles: Vec<&str> = msgs.iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert!(roles.contains(&"user"));
    assert!(roles.iter().filter(|r| **r == "assistant").count() >= 2);
    assert!(
        msgs.iter().any(|m| m["content"] == user_text),
        "user content must be re-listable"
    );

    let (status, finished) = json_request(
        &app,
        "POST",
        &format!("/api/v1/interviews/{session_id}/finish"),
        Some(token),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finished}");
    assert_eq!(finished["status"], "finished");

    let (status, messages_after) = json_request(
        &app,
        "GET",
        &format!("/api/v1/interviews/{session_id}/messages"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        messages_after.as_array().unwrap().len(),
        msgs.len(),
        "finish must not drop messages"
    );

    cleanup_user(&state.pool, user_id).await;
}

#[test]
fn default_chapter_titles_match_mvp_spec() {
    assert_eq!(
        DEFAULT_CHAPTER_TITLES,
        &[
            "童年与家庭",
            "求学经历",
            "青年时代",
            "工作与事业",
            "婚姻与家庭",
            "人生转折",
            "子女与家庭生活",
            "退休与晚年",
            "我想留下的话",
        ]
    );
}

#[tokio::test]
async fn admin_setup_login_overview_and_ai_test() {
    if !database_configured() {
        eprintln!("skip: DATABASE_URL/TEST_DATABASE_URL not set");
        return;
    }
    prepare_test_env();
    let (app, state) = memoir_server::app_from_env().await.expect("app");

    // Isolate admin tests inside memoir_test only.
    sqlx::query("DELETE FROM admin_accounts")
        .execute(&state.pool)
        .await
        .expect("clear admins in test db");

    let (status, st) = json_request(&app, "GET", "/api/v1/admin/setup-status", None, None).await;
    assert_eq!(status, StatusCode::OK, "{st}");
    assert_eq!(st["needs_setup"], true);

    let username = format!("admin_{}", &Uuid::new_v4().to_string()[..8]);
    let password = "RealPass9!";

    let (status, setup) = json_request(
        &app,
        "POST",
        "/api/v1/admin/setup",
        None,
        Some(json!({
            "username": username,
            "password": password,
            "display_name": "test_admin"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{setup}");
    assert_eq!(setup["username"], username);
    assert!(!setup["token"].as_str().unwrap_or("").is_empty());

    let (status, again) = json_request(
        &app,
        "POST",
        "/api/v1/admin/setup",
        None,
        Some(json!({
            "username": "another",
            "password": "AnotherPass9!"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");

    let (status, st2) = json_request(&app, "GET", "/api/v1/admin/setup-status", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(st2["needs_setup"], false);

    let (status, bad) = json_request(
        &app,
        "POST",
        "/api/v1/admin/login",
        None,
        Some(json!({ "username": username, "password": "wrong-password-xyz" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{bad}");

    let (status, auth) = json_request(
        &app,
        "POST",
        "/api/v1/admin/login",
        None,
        Some(json!({ "username": username, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{auth}");
    let token = auth["token"].as_str().expect("admin token");

    let (status, me) = json_request(&app, "GET", "/api/v1/admin/me", Some(token), None).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["username"], username);

    let (status, overview) =
        json_request(&app, "GET", "/api/v1/admin/overview", Some(token), None).await;
    assert_eq!(status, StatusCode::OK, "{overview}");
    assert!(overview["users"].as_i64().unwrap_or(-1) >= 0);
    assert!(overview["ai"]["mode"].as_str().is_some());

    let (status, users) = json_request(&app, "GET", "/api/v1/admin/users", Some(token), None).await;
    assert_eq!(status, StatusCode::OK, "{users}");
    assert!(users.as_array().is_some());

    let (status, memoirs) =
        json_request(&app, "GET", "/api/v1/admin/memoirs", Some(token), None).await;
    assert_eq!(status, StatusCode::OK, "{memoirs}");

    let (status, test) = json_request(
        &app,
        "POST",
        "/api/v1/admin/ai-config/test",
        Some(token),
        Some(json!({ "prompt": "hello" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{test}");
    assert_eq!(test["ok"], true);
    assert!(!test["reply"].as_str().unwrap_or("").is_empty());

    let (status, usage) =
        json_request(&app, "GET", "/api/v1/admin/ai-usage", Some(token), None).await;
    assert_eq!(status, StatusCode::OK, "{usage}");
    assert!(usage["summary"]["calls"].as_i64().unwrap_or(0) >= 1);

    let (status, chg) = json_request(
        &app,
        "POST",
        "/api/v1/admin/change-password",
        Some(token),
        Some(json!({
            "current_password": password,
            "new_password": "NewRealPass9!"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{chg}");

    let (status, re) = json_request(
        &app,
        "POST",
        "/api/v1/admin/login",
        None,
        Some(json!({ "username": username, "password": "NewRealPass9!" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{re}");

    std::env::set_var("ADMIN_RECOVERY_SECRET", "test-recovery-secret");
    let (app2, _) = memoir_server::app_from_env().await.expect("app2");
    let (status, st3) = json_request(&app2, "GET", "/api/v1/admin/setup-status", None, None).await;
    assert_eq!(status, StatusCode::OK, "{st3}");
    assert_eq!(st3["recovery_enabled"], true);

    let (status, reset) = json_request(
        &app2,
        "POST",
        "/api/v1/admin/reset-password",
        None,
        Some(json!({
            "username": username,
            "recovery_secret": "test-recovery-secret",
            "new_password": "ResetPass9!"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reset}");

    let (status, re2) = json_request(
        &app2,
        "POST",
        "/api/v1/admin/login",
        None,
        Some(json!({ "username": username, "password": "ResetPass9!" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{re2}");

    // Leave test DB clean of admin fixtures.
    sqlx::query("DELETE FROM admin_accounts")
        .execute(&state.pool)
        .await
        .ok();
    sqlx::query("TRUNCATE llm_usage_logs RESTART IDENTITY")
        .execute(&state.pool)
        .await
        .ok();
}
