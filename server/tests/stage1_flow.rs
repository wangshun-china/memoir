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
    // Progress fields present even when empty; default list omits full content.
    assert_eq!(listed_arr[0]["has_interview"], false);
    assert_eq!(listed_arr[0]["has_draft"], false);
    assert_eq!(listed_arr[0]["message_count"], 0);
    assert!(listed_arr[0]["content"].is_null() || listed_arr[0]["content"] == "");

    // Delete memoir (and cascade).
    let (status, _) = json_request(
        &app,
        "DELETE",
        &format!("/api/v1/memoirs/{memoir_id}"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, again) = json_request(
        &app,
        "GET",
        &format!("/api/v1/memoirs/{memoir_id}"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{again}");

    cleanup_user(&state.pool, user_id).await;
}

#[tokio::test]
async fn nonlinear_story_box_flow_is_traceable_and_organizable() {
    if !database_configured() {
        eprintln!("skip: DATABASE_URL/TEST_DATABASE_URL not set");
        return;
    }
    prepare_test_env();
    let (app, state) = memoir_server::app_from_env().await.expect("app");
    let openid = format!("test-story-box-{}", Uuid::new_v4());

    let (status, auth) = json_request(
        &app,
        "POST",
        "/api/v1/auth/dev-login",
        None,
        Some(json!({ "nickname": "story_teller", "openid": openid })),
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
        Some(json!({ "subject_name": "故事测试者", "birth_year": 1950 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "memoir: {memoir}");
    let memoir_id = memoir["id"].as_str().unwrap();

    let (status, started) = json_request(
        &app,
        "POST",
        &format!("/api/v1/memoirs/{memoir_id}/interviews"),
        Some(token),
        Some(json!({ "topic": "自由回忆", "force_new": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "start: {started}");
    assert_eq!(started["session"]["topic"], "自由回忆");
    let session_id = started["session"]["id"].as_str().unwrap();
    assert!(started["opening_message"]["question_type"]
        .as_str()
        .unwrap()
        .starts_with("free_"));

    let (status, posted) = json_request(
        &app,
        "POST",
        &format!("/api/v1/interviews/{session_id}/messages"),
        Some(token),
        Some(json!({
            "content": "我十岁左右住在河边，父亲每天骑一辆旧自行车送我去学校。",
            "action": "normal"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "post: {posted}");

    let story_uri = format!("/api/v1/interviews/{session_id}/stories");
    let (status, story) = json_request(&app, "POST", &story_uri, Some(token), Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "extract: {story}");
    assert_eq!(story["status"], "draft");
    assert!(story["source_count"].as_i64().unwrap_or(0) >= 3);
    let story_id = story["id"].as_str().unwrap();

    // Extraction is idempotent for a short interview session.
    let (status, again) =
        json_request(&app, "POST", &story_uri, Some(token), Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "extract again: {again}");
    assert_eq!(again["id"], story_id);

    let (status, confirmed) = json_request(
        &app,
        "POST",
        &format!("/api/v1/stories/{story_id}/confirm"),
        Some(token),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "confirm: {confirmed}");
    assert_eq!(confirmed["status"], "confirmed");

    let (status, organized) = json_request(
        &app,
        "POST",
        &format!("/api/v1/memoirs/{memoir_id}/organize"),
        Some(token),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "organize: {organized}");
    assert_eq!(organized["story_count"], 1);
    assert_eq!(organized["timeline"].as_array().unwrap().len(), 1);
    assert_eq!(organized["chapters"].as_array().unwrap().len(), 1);

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
    assert_eq!(session_body["resumed"], false);

    // Re-enter same memoir+topic must resume, not create a fresh empty chat.
    let (status, resume_body) = json_request(
        &app,
        "POST",
        &format!("/api/v1/memoirs/{memoir_id}/interviews"),
        Some(token),
        Some(json!({ "topic": "童年与家庭" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resume_body}");
    assert_eq!(resume_body["resumed"], true);
    assert_eq!(resume_body["session"]["id"], session_id);

    // Continue-by-id must keep the same session (home「继续采访」路径).
    let (status, cont) = json_request(
        &app,
        "POST",
        &format!("/api/v1/interviews/{session_id}/continue"),
        Some(token),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cont}");
    assert_eq!(cont["resumed"], true);
    assert_eq!(cont["session"]["id"], session_id);

    // List memoirs exposes has_interview + continue_session_id for button split.
    let (status, listed) = json_request(&app, "GET", "/api/v1/memoirs", Some(token), None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let item = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == memoir_id)
        .expect("memoir in list");
    assert_eq!(item["has_interview"], true);
    assert_eq!(item["continue_session_id"], session_id);

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
    assert_eq!(msg_resp["user_turn_count"], 1);
    assert!(msg_resp.get("generation_started").is_some());
    assert!(msg_resp.get("generation_status").is_some());
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

    // Manual generate chapter draft and persist to chapters.content
    let (status, gen) = json_request(
        &app,
        "POST",
        &format!("/api/v1/interviews/{session_id}/generate"),
        Some(token),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gen}");
    assert_eq!(gen["chapter"]["title"], "童年与家庭");
    assert_eq!(gen["chapter"]["status"], "draft");
    let content = gen["chapter"]["content"].as_str().unwrap_or("");
    assert!(!content.is_empty(), "generated content must be stored");

    let (status, chapters) = json_request(
        &app,
        "GET",
        &format!("/api/v1/memoirs/{memoir_id}/chapters?include_content=true"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{chapters}");
    let ch0 = &chapters.as_array().unwrap()[0];
    assert_eq!(ch0["title"], "童年与家庭");
    assert!(!ch0["content"].as_str().unwrap_or("").is_empty());
    assert_eq!(
        ch0["has_interview"], true,
        "reader must not show 待采访 after dialogue"
    );
    assert_eq!(ch0["has_draft"], true);
    assert!(ch0["message_count"].as_i64().unwrap_or(0) >= 1);

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

#[tokio::test]
async fn password_login_auto_register_and_admin_username() {
    if !database_configured() {
        eprintln!("skip: DATABASE_URL/TEST_DATABASE_URL not set");
        return;
    }
    prepare_test_env();
    let (app, state) = memoir_server::app_from_env().await.expect("app");
    let uname = format!("u_{}", &Uuid::new_v4().to_string()[..8]);

    let (status, reg) = json_request(
        &app,
        "POST",
        "/api/v1/auth/password",
        None,
        Some(json!({ "username": uname, "password": "password123" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reg}");
    assert_eq!(reg["registered"], true);
    assert_eq!(reg["is_admin"], false);
    assert!(!reg["token"].as_str().unwrap_or("").is_empty());
    let user_id = Uuid::parse_str(reg["user_id"].as_str().unwrap()).unwrap();

    let (status, again) = json_request(
        &app,
        "POST",
        "/api/v1/auth/password",
        None,
        Some(json!({ "username": uname, "password": "password123" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert!(again.get("registered").is_none() || again["registered"] == false);

    let (status, bad) = json_request(
        &app,
        "POST",
        "/api/v1/auth/password",
        None,
        Some(json!({ "username": uname, "password": "wrongpass99" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{bad}");

    // Reserved admin username
    let (status, admin) = json_request(
        &app,
        "POST",
        "/api/v1/auth/password",
        None,
        Some(json!({ "username": "wangshun", "password": "password123" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{admin}");
    assert_eq!(admin["is_admin"], true);
    let admin_token = admin["token"].as_str().unwrap();
    let (status, overview) =
        json_request(&app, "GET", "/api/v1/admin/overview", Some(admin_token), None).await;
    assert_eq!(status, StatusCode::OK, "{overview}");

    // Forgot password with trial recovery key
    let (status, reset) = json_request(
        &app,
        "POST",
        "/api/v1/auth/reset-password",
        None,
        Some(json!({
            "username": uname,
            "recovery_key": "wangshun",
            "new_password": "newpass456"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reset}");
    let (status, bad_key) = json_request(
        &app,
        "POST",
        "/api/v1/auth/reset-password",
        None,
        Some(json!({
            "username": uname,
            "recovery_key": "wrong",
            "new_password": "newpass456"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{bad_key}");
    let (status, with_new) = json_request(
        &app,
        "POST",
        "/api/v1/auth/password",
        None,
        Some(json!({ "username": uname, "password": "newpass456" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{with_new}");

    cleanup_user(&state.pool, user_id).await;
    if let Ok(aid) = Uuid::parse_str(admin["user_id"].as_str().unwrap_or("")) {
        cleanup_user(&state.pool, aid).await;
    }
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

#[tokio::test]
async fn admin_bind_wechat_promotes_wechat_login() {
    if !database_configured() {
        eprintln!("skip: DATABASE_URL/TEST_DATABASE_URL not set");
        return;
    }
    prepare_test_env();
    let (app, state) = memoir_server::app_from_env().await.expect("app");

    // Create our own is_admin user (unique username) so this test does not race
    // with other tests that mutate the shared wangshun / admin_accounts fixtures.
    let uname = format!("adm_bind_{}", &Uuid::new_v4().to_string()[..8]);
    let password = "BindPass9!";
    let hash = memoir_server::admin::password::hash_password(password).expect("hash");
    let (admin_id,): (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO users (username, password_hash, nickname, is_admin)
        VALUES ($1, $2, $3, TRUE)
        RETURNING id
        "#,
    )
    .bind(&uname)
    .bind(&hash)
    .bind("bind-test-admin")
    .fetch_one(&state.pool)
    .await
    .expect("insert admin user");

    let (status, login) = json_request(
        &app,
        "POST",
        "/api/v1/auth/password",
        None,
        Some(json!({ "username": uname, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login}");
    assert_eq!(login["is_admin"], true);
    let admin_token = login["token"].as_str().unwrap();

    // Bind a WeChat openid (dev mode) as admin.
    let openid = format!("bind_openid_{}", Uuid::new_v4().to_string().replace('-', ""));
    let (status, bind) = json_request(
        &app,
        "POST",
        "/api/v1/admin/bind-wechat",
        Some(admin_token),
        Some(json!({ "openid": openid })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bind}");
    assert_eq!(bind["bound"], true);
    assert_eq!(bind["promoted"], false);
    assert!(bind["openid_masked"].as_str().unwrap_or("").contains('…'));

    // WeChat login for that openid now yields an admin session.
    let (status, wechat) = json_request(
        &app,
        "POST",
        "/api/v1/auth/dev-login",
        None,
        Some(json!({ "openid": openid, "nickname": "微信管理员" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{wechat}");
    assert_eq!(wechat["is_admin"], true);
    let wechat_token = wechat["token"].as_str().unwrap();

    let (status, overview) = json_request(
        &app,
        "GET",
        "/api/v1/admin/overview",
        Some(wechat_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{overview}");

    // Rebinding the same openid is idempotent and reports promoted.
    let (status, rebind) = json_request(
        &app,
        "POST",
        "/api/v1/admin/bind-wechat",
        Some(admin_token),
        Some(json!({ "openid": openid })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rebind}");
    assert_eq!(rebind["promoted"], true);

    // Without admin auth the endpoint is rejected.
    let (status, denied) = json_request(
        &app,
        "POST",
        "/api/v1/admin/bind-wechat",
        None,
        Some(json!({ "openid": "nobody" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{denied}");

    // Cleanup both users.
    cleanup_user(&state.pool, admin_id).await;
    cleanup_user(
        &state.pool,
        wechat["user_id"].as_str().unwrap().parse().unwrap(),
    )
    .await;
}
