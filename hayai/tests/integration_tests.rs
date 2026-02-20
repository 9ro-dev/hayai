use hayai::prelude::*;
use hayai::openapi;
use serde::{Serialize, Deserialize};
use schemars::JsonSchema;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ApiModel)]
struct TestUser {
    id: i64,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ApiModel)]
struct CreateTestUser {
    #[validate(min_length = 1, max_length = 50)]
    name: String,
    #[validate(email)]
    email: String,
}

// ---- Validation Tests ----

#[test]
fn test_validation_passes() {
    let user = CreateTestUser {
        name: "Alice".into(),
        email: "alice@example.com".into(),
    };
    assert!(user.validate().is_ok());
}

#[test]
fn test_validation_min_length() {
    let user = CreateTestUser {
        name: "".into(),
        email: "alice@example.com".into(),
    };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("must be at least 1")));
}

#[test]
fn test_validation_max_length() {
    let user = CreateTestUser {
        name: "a".repeat(51),
        email: "alice@example.com".into(),
    };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("must be at most 50")));
}

#[test]
fn test_validation_email() {
    let user = CreateTestUser {
        name: "Alice".into(),
        email: "notanemail".into(),
    };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("valid email")));
}

#[test]
fn test_validation_multiple_errors() {
    let user = CreateTestUser {
        name: "".into(),
        email: "bad".into(),
    };
    let err = user.validate().unwrap_err();
    assert_eq!(err.len(), 2);
}

// ---- Schema Tests ----

#[test]
fn test_schema_generation() {
    // Check that SchemaInfo is registered via inventory
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let test_user_schema = schemas.iter().find(|s| s.name == "CreateTestUser");
    assert!(test_user_schema.is_some(), "CreateTestUser schema should be registered");
    
    let schema = (test_user_schema.unwrap().schema_fn)();
    assert_eq!(schema.type_name, "object");
    assert!(schema.properties.contains_key("name"));
    assert!(schema.properties.contains_key("email"));
    
    // Check validation constraints in schema
    let name_prop = &schema.properties["name"];
    assert_eq!(name_prop.min_length, Some(1));
    assert_eq!(name_prop.max_length, Some(50));
    
    let email_prop = &schema.properties["email"];
    assert_eq!(email_prop.format.as_deref(), Some("email"));
}

// ---- Route Registration Tests ----

struct MockDb;

#[get("/test/{id}")]
async fn test_get_route(id: i64, db: Dep<MockDb>) -> TestUser {
    TestUser { id, name: "test".into() }
}

#[test]
fn test_route_info_registered() {
    let info = test_get_route();
    assert_eq!(info.path, "/test/{id}");
    assert_eq!(info.method, "GET");
    assert_eq!(info.response_type_name, "TestUser");
    assert_eq!(info.parameters.len(), 1);
    assert_eq!(info.parameters[0].name, "id");
}

// ---- App Builder Tests ----

#[test]
fn test_app_builder() {
    let app = HayaiApp::new()
        .dep(MockDb)
        .route(test_get_route);
    // If we got here, builder works
    assert!(true);
}

// ---- API Error Tests ----

#[test]
fn test_api_error_serialization() {
    let err = hayai::ApiError::validation_error(vec!["field: bad".into()]);
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["error"], "Validation failed");
    assert_eq!(json["details"][0], "field: bad");
}

#[test]
fn test_api_error_bad_request() {
    let err = hayai::ApiError::bad_request("oops".into());
    assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
}
