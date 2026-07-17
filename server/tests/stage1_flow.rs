//! Integration tests for Stage-1 memoir + interview handlers.
//! Requires DATABASE_URL pointing at a Postgres instance.
//!
//! These tests drive the same router the binary mounts (memoir_server::build_router /
//! app_from_env), not a reimplemented mock of the unit under test.

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use memoir_server::memoirs::DEFAULT_CHAPTER_TITLES;
use serde_json::{json, Value};
use tower::ServiceExt;

fn database_url_set() -> bool {
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .is_some()
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
    if !database_url_set() {
        eprintln!("skip: DATABASE_URL not set");
        return;
    }
    dotenvy::dotenv().ok();
    let (app, _) = memoir_server::app_from_env().await.expect("app");
    let (status, json) = json_request(&app, "GET", "/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "memoir-server");
}

#[tokio::test]
async fn create_memoir_seeds_nine_default_chapters() {
    if !database_url_set() {
        eprintln!("skip: DATABASE_URL not set");
        return;
    }
    dotenvy::dotenv().ok();
    let (app, _) = memoir_server::app_from_env().await.expect("app");

    let (status, auth) = json_request(
        &app,
        "POST",
        "/api/v1/auth/dev-login",
        None,
        Some(json!({ "nickname": "测试创建者", "openid": format!("test-memoir-{}", uuid::Uuid::new_v4()) })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "auth: {auth}");
    let token = auth["token"].as_str().expect("token");

    let (status, memoir) = json_request(
        &app,
        "POST",
        "/api/v1/memoirs",
        Some(token),
        Some(json!({
            "subject_name": "王大爷",
            "birth_year": 1948,
            "birth_place": "河北",
            "preferred_name": "王大爷",
            "creator_relation": "儿子"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create memoir: {memoir}");
    assert_eq!(memoir["subject_name"], "王大爷");
    assert!(memoir["title"].as_str().unwrap().contains("王大爷"));

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
}

#[tokio::test]
async fn interview_message_round_trip_and_finish() {
    if !database_url_set() {
        eprintln!("skip: DATABASE_URL not set");
        return;
    }
    dotenvy::dotenv().ok();
    let (app, _) = memoir_server::app_from_env().await.expect("app");

    let (status, auth) = json_request(
        &app,
        "POST",
        "/api/v1/auth/dev-login",
        None,
        Some(json!({ "nickname": "采访测试", "openid": format!("test-iv-{}", uuid::Uuid::new_v4()) })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = auth["token"].as_str().unwrap();

    let (status, memoir) = json_request(
        &app,
        "POST",
        "/api/v1/memoirs",
        Some(token),
        Some(json!({ "subject_name": "李奶奶" })),
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

    let user_text = "我小时候住在村口的土坯房，冬天很冷。";
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
    // opening assistant + user + assistant follow-up
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

    // Prior messages must remain after finish.
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
    if !database_url_set() {
        eprintln!("skip: DATABASE_URL not set");
        return;
    }
    dotenvy::dotenv().ok();
    let (app, state) = memoir_server::app_from_env().await.expect("app");

    // Isolate: wipe admin accounts for this test (real table, no mock password).
    sqlx::query("DELETE FROM admin_accounts")
        .execute(&state.pool)
        .await
        .expect("clear admins");

    let (status, st) =
        json_request(&app, "GET", "/api/v1/admin/setup-status", None, None).await;
    assert_eq!(status, StatusCode::OK, "{st}");
    assert_eq!(st["needs_setup"], true);

    let username = format!("admin_{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let password = "RealPass9!";

    let (status, setup) = json_request(
        &app,
        "POST",
        "/api/v1/admin/setup",
        None,
        Some(json!({
            "username": username,
            "password": password,
            "display_name": "测试管理员"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{setup}");
    assert_eq!(setup["username"], username);
    assert!(!setup["token"].as_str().unwrap_or("").is_empty());

    // Second setup must fail.
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

    let (status, st2) =
        json_request(&app, "GET", "/api/v1/admin/setup-status", None, None).await;
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

    let (status, users) =
        json_request(&app, "GET", "/api/v1/admin/users", Some(token), None).await;
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
        Some(json!({ "prompt": "你好" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{test}");
    assert_eq!(test["ok"], true);
    assert!(!test["reply"].as_str().unwrap_or("").is_empty());

    let (status, usage) =
        json_request(&app, "GET", "/api/v1/admin/ai-usage", Some(token), None).await;
    assert_eq!(status, StatusCode::OK, "{usage}");
    assert!(usage["summary"]["calls"].as_i64().unwrap_or(0) >= 1);

    // Change password with real hash verify.
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
}
