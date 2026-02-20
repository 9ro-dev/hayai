use hayai::prelude::*;
use hayai::axum;
use serde::Serialize;
use serde_json::Value;

/// Custom claims type for testing
#[derive(Clone, Debug, Serialize)]
struct TestClaims {
    user_id: i64,
    name: String,
}

/// Custom auth validator for testing - validates both Bearer tokens and X-API-Key
#[derive(Clone)]
struct TestAuthValidator;

#[async_trait::async_trait]
impl AuthValidator for TestAuthValidator {
    type Credentials = TestClaims;

    async fn validate(&self, token: &str) -> Result<Self::Credentials, ApiError> {
        match token {
            "valid-token-42" => Ok(TestClaims { user_id: 42, name: "User 42".into() }),
            "valid-token-99" => Ok(TestClaims { user_id: 99, name: "User 99".into() }),
            "api-key-123" => Ok(TestClaims { user_id: 100, name: "API User".into() }),
            _ => Err(ApiError::unauthorized("Invalid token")),
        }
    }
}

#[derive(serde::Serialize, schemars::JsonSchema)]
struct UserProfile {
    id: i64,
    name: String,
}

/// Protected route that requires authentication
#[get("/me")]
#[security("bearer")]
async fn get_me(auth: Auth<TestAuthValidator>) -> UserProfile {
    UserProfile {
        id: auth.user.user_id,
        name: auth.user.name.clone(),
    }
}

/// Public route that doesn't require authentication
#[get("/health")]
async fn health_check() -> UserProfile {
    UserProfile { id: 0, name: "healthy".into() }
}

/// Another protected route
#[get("/admin")]
#[security("bearer")]
async fn admin_route(auth: Auth<TestAuthValidator>) -> String {
    format!("Admin access for user {}", auth.user.user_id)
}

fn create_router() -> HayaiRouter {
    HayaiRouter::new("/api")
        .security("bearer")
        .route(__HAYAI_ROUTE_GET_ME)
        .route(__HAYAI_ROUTE_ADMIN_ROUTE)
}

fn create_public_router() -> HayaiRouter {
    HayaiRouter::new("")
        .route(__HAYAI_ROUTE_HEALTH_CHECK)
}

async fn spawn_app() -> String {
    let app = HayaiApp::new()
        .title("Auth Test")
        .version("1.0.0")
        .bearer_auth()
        .dep(TestAuthValidator)
        .include(create_router())
        .include(create_public_router())
        .into_router();
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    format!("http://{}", addr)
}

#[tokio::test]
async fn test_no_token_401() {
    let base = spawn_app().await;
    let r = reqwest::get(format!("{base}/api/me")).await.unwrap();
    assert_eq!(r.status(), 401);
    
    // Verify error response
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["error"], "Missing authorization header");
}

#[tokio::test]
async fn test_invalid_token_401() {
    let base = spawn_app().await;
    let c = reqwest::Client::new();
    let r = c.get(format!("{base}/api/me"))
        .header("Authorization", "Bearer bad")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    
    // Verify error response
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["error"], "Invalid token");
}

#[tokio::test]
async fn test_wrong_format_401() {
    let base = spawn_app().await;
    let c = reqwest::Client::new();
    let r = c.get(format!("{base}/api/me"))
        .header("Authorization", "Basic x")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn test_valid_token_200() {
    let base = spawn_app().await;
    let c = reqwest::Client::new();
    let r = c.get(format!("{base}/api/me"))
        .header("Authorization", "Bearer valid-token-42")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    
    let b: Value = r.json().await.unwrap();
    assert_eq!(b["id"], 42);
    assert_eq!(b["name"], "User 42");
}

#[tokio::test]
async fn test_different_token() {
    let base = spawn_app().await;
    let c = reqwest::Client::new();
    let r = c.get(format!("{base}/api/me"))
        .header("Authorization", "Bearer valid-token-99")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    
    let b: Value = r.json().await.unwrap();
    assert_eq!(b["id"], 99);
}

#[tokio::test]
async fn test_unsecured_no_auth_needed() {
    let base = spawn_app().await;
    let r = reqwest::get(format!("{base}/health")).await.unwrap();
    assert_eq!(r.status(), 200);
    
    let b: Value = r.json().await.unwrap();
    assert_eq!(b["name"], "healthy");
}

#[tokio::test]
async fn test_openapi_security_on_secured_route() {
    let base = spawn_app().await;
    let b: Value = reqwest::get(format!("{base}/openapi.json")).await.unwrap().json().await.unwrap();
    
    let op = &b["paths"]["/api/me"]["get"];
    let sec = op["security"].as_array().expect("should have security");
    assert!(sec.iter().any(|s| s.get("bearerAuth").is_some()));
}

#[tokio::test]
async fn test_openapi_401_on_secured_route() {
    let base = spawn_app().await;
    let b: Value = reqwest::get(format!("{base}/openapi.json")).await.unwrap().json().await.unwrap();
    
    // Check that 401 response is defined for secured routes
    assert!(b["paths"]["/api/me"]["get"]["responses"]["401"].is_object());
}

#[tokio::test]
async fn test_openapi_no_security_on_public_route() {
    let base = spawn_app().await;
    let b: Value = reqwest::get(format!("{base}/openapi.json")).await.unwrap().json().await.unwrap();
    
    let op = &b["paths"]["/health"]["get"];
    // Public routes should either have no security or empty security array
    assert!(op.get("security").is_none() || op["security"].as_array().map(|a| a.is_empty()).unwrap_or(true));
}

#[tokio::test]
async fn test_openapi_bearer_scheme() {
    let base = spawn_app().await;
    let b: Value = reqwest::get(format!("{base}/openapi.json")).await.unwrap().json().await.unwrap();
    
    let s = &b["components"]["securitySchemes"]["bearerAuth"];
    assert_eq!(s["type"], "http");
    assert_eq!(s["scheme"], "bearer");
}

#[tokio::test]
async fn test_swagger_ui() {
    let base = spawn_app().await;
    let r = reqwest::get(format!("{base}/docs")).await.unwrap();
    assert_eq!(r.status(), 200);
    let text = r.text().await.unwrap();
    assert!(text.contains("scalar") || text.contains("swagger"));
}

#[tokio::test]
async fn test_x_api_key_header() {
    let base = spawn_app().await;
    let c = reqwest::Client::new();
    
    // Test that X-API-Key header works with valid key
    let r = c.get(format!("{base}/api/me"))
        .header("X-Api-Key", "api-key-123")
        .send()
        .await
        .unwrap();
    
    // Should succeed with valid API key
    assert_eq!(r.status(), 200);
    
    let b: Value = r.json().await.unwrap();
    assert_eq!(b["id"], 100);
    assert_eq!(b["name"], "API User");
}

#[tokio::test]
async fn test_x_api_key_header_invalid() {
    let base = spawn_app().await;
    let c = reqwest::Client::new();
    
    // Test that X-API-Key header fails with invalid key
    let r = c.get(format!("{base}/api/me"))
        .header("X-Api-Key", "invalid-key")
        .send()
        .await
        .unwrap();
    
    // Should fail with invalid API key
    assert_eq!(r.status(), 401);
}
