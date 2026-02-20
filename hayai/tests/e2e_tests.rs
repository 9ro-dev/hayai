use hayai::prelude::*;
use hayai::axum;
use serde_json::Value;

// --- App setup (mirrors the example app) ---

#[api_model]
#[derive(Debug, Clone)]
struct User {
    id: i64,
    name: String,
    email: String,
}

#[api_model]
#[derive(Debug, Clone)]
struct CreateUser {
    #[validate(min_length = 1, max_length = 100)]
    name: String,
    #[validate(email)]
    email: String,
}

#[api_model]
#[derive(Debug, Clone)]
struct Address {
    city: String,
    country: String,
}

#[api_model]
#[derive(Debug, Clone)]
struct UserWithAddress {
    name: String,
    address: Address,
    tags: Vec<String>,
    nickname: Option<String>,
}

struct Database;
impl Database {
    async fn get_user(&self, id: i64) -> Option<User> {
        Some(User { id, name: "Alice".into(), email: "alice@example.com".into() })
    }
    async fn create_user(&self, input: &CreateUser) -> User {
        User { id: 1, name: input.name.clone(), email: input.email.clone() }
    }
}

#[get("/users/{id}")]
async fn get_user(id: i64, db: Dep<Database>) -> User {
    db.get_user(id).await.unwrap()
}

#[post("/users")]
async fn create_user(body: CreateUser, db: Dep<Database>) -> User {
    db.create_user(&body).await
}

// --- Helper ---

async fn spawn_app() -> String {
    let app = HayaiApp::new()
        .title("Test API")
        .version("0.1.0")
        .dep(Database)
        .into_router();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

// --- Tests ---

#[tokio::test]
async fn test_get_user_returns_200() {
    let base = spawn_app().await;
    let resp = reqwest::get(format!("{base}/users/42")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["id"], 42);
    assert_eq!(body["name"], "Alice");
    assert_eq!(body["email"], "alice@example.com");
}

#[tokio::test]
async fn test_create_user_valid() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("{base}/users"))
        .json(&serde_json::json!({"name": "Bob", "email": "bob@example.com"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["name"], "Bob");
    assert_eq!(body["email"], "bob@example.com");
}

#[tokio::test]
async fn test_create_user_empty_name_returns_422() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("{base}/users"))
        .json(&serde_json::json!({"name": "", "email": "bob@example.com"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 422);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Validation failed");
    assert!(!body["details"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_create_user_invalid_email_returns_422() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("{base}/users"))
        .json(&serde_json::json!({"name": "Bob", "email": "notanemail"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 422);
    let body: Value = resp.json().await.unwrap();
    let details = body["details"].as_array().unwrap();
    assert!(details.iter().any(|d| d.as_str().unwrap().contains("email")));
}

#[tokio::test]
async fn test_create_user_malformed_json_returns_400() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("{base}/users"))
        .header("content-type", "application/json")
        .body("{not json")
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_openapi_spec() {
    let base = spawn_app().await;
    let resp = reqwest::get(format!("{base}/openapi.json")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"].as_object().unwrap().contains_key("/users/{id}"));
    assert!(body["paths"].as_object().unwrap().contains_key("/users"));
    assert!(body["components"]["schemas"].as_object().unwrap().contains_key("User"));
}

#[tokio::test]
async fn test_openapi_nested_schemas() {
    let base = spawn_app().await;
    let resp = reqwest::get(format!("{base}/openapi.json")).await.unwrap();
    let body: Value = resp.json().await.unwrap();
    let schemas = &body["components"]["schemas"];
    
    // Address should exist as a nested schema
    assert!(schemas.get("Address").is_some(), "Address schema should be in components/schemas");
    assert_eq!(schemas["Address"]["type"], "object");
    assert!(schemas["Address"]["properties"]["city"]["type"] == "string");
    
    // UserWithAddress should have $ref for address
    assert!(schemas.get("UserWithAddress").is_some());
    assert_eq!(schemas["UserWithAddress"]["properties"]["address"]["$ref"], "#/components/schemas/Address");
    
    // tags should be array
    assert_eq!(schemas["UserWithAddress"]["properties"]["tags"]["type"], "array");
    assert_eq!(schemas["UserWithAddress"]["properties"]["tags"]["items"]["type"], "string");
    
    // nickname should be nullable (anyOf)
    assert!(schemas["UserWithAddress"]["properties"]["nickname"].get("anyOf").is_some());
}

#[tokio::test]
async fn test_docs_returns_swagger_html() {
    let base = spawn_app().await;
    let resp = reqwest::get(format!("{base}/docs")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.to_lowercase().contains("swagger"));
}

#[tokio::test]
async fn test_nonexistent_route_returns_404() {
    let base = spawn_app().await;
    let resp = reqwest::get(format!("{base}/nonexistent")).await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_dep_injection_works() {
    // Verify that Dep<Database> is properly injected by calling an endpoint that uses it
    let base = spawn_app().await;
    let resp = reqwest::get(format!("{base}/users/1")).await.unwrap();
    assert_eq!(resp.status(), 200);
    // If dep injection failed, this would have panicked on the server side and returned 500
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["id"], 1);
}
