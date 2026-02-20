use hayai::prelude::*;
use hayai::openapi;
use std::collections::HashMap;

#[api_model]
#[derive(Debug, Clone)]
struct TestUser {
    id: i64,
    name: String,
}

#[api_model]
#[derive(Debug, Clone)]
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
fn test_validation_email_missing_at() {
    let user = CreateTestUser {
        name: "Alice".into(),
        email: "notanemail".into(),
    };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("valid email")));
}

#[test]
fn test_validation_email_at_start() {
    let user = CreateTestUser {
        name: "Alice".into(),
        email: "@example.com".into(),
    };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("valid email")));
}

#[test]
fn test_validation_email_at_end() {
    let user = CreateTestUser {
        name: "Alice".into(),
        email: "user@".into(),
    };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("valid email")));
}

#[test]
fn test_validation_email_no_dot_in_domain() {
    let user = CreateTestUser {
        name: "Alice".into(),
        email: "user@localhost".into(),
    };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("valid email")));
}

#[test]
fn test_validation_email_multiple_at() {
    let user = CreateTestUser {
        name: "Alice".into(),
        email: "user@@example.com".into(),
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
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let test_user_schema = schemas.iter().find(|s| s.name == "CreateTestUser");
    assert!(test_user_schema.is_some(), "CreateTestUser schema should be registered");
    
    let schema = (test_user_schema.unwrap().schema_fn)();
    assert_eq!(schema.type_name, "object");
    assert!(schema.properties.contains_key("name"));
    assert!(schema.properties.contains_key("email"));
    
    let name_prop = &schema.properties["name"];
    assert_eq!(name_prop.min_length, Some(1));
    assert_eq!(name_prop.max_length, Some(50));
    
    let email_prop = &schema.properties["email"];
    assert_eq!(email_prop.format.as_deref(), Some("email"));
}

// ---- Nested Struct / Vec / Option Schema Tests ----

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

#[test]
fn test_nested_struct_ref() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "UserWithAddress").unwrap();
    let schema = (info.schema_fn)();
    let addr_prop = &schema.properties["address"];
    assert!(addr_prop.ref_path.is_some(), "address should have $ref");
    assert_eq!(addr_prop.ref_path.as_deref().unwrap(), "#/components/schemas/Address");
}

#[test]
fn test_vec_string_schema() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "UserWithAddress").unwrap();
    let schema = (info.schema_fn)();
    let tags_prop = &schema.properties["tags"];
    assert_eq!(tags_prop.type_name, "array");
    assert!(tags_prop.items.is_some(), "tags should have items");
    assert_eq!(tags_prop.items.as_ref().unwrap().type_name, "string");
}

#[test]
fn test_option_nullable() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "UserWithAddress").unwrap();
    let schema = (info.schema_fn)();
    let nick_prop = &schema.properties["nickname"];
    assert!(nick_prop.nullable, "nickname should be nullable");
    assert_eq!(nick_prop.type_name, "string");
}

#[test]
fn test_nested_definitions_collected() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "UserWithAddress").unwrap();
    let nested = (info.nested_fn)();
    assert!(nested.contains_key("Address"), "Address should be in nested definitions");
    let addr_schema = &nested["Address"];
    assert!(addr_schema.properties.contains_key("city"));
    assert!(addr_schema.properties.contains_key("country"));
}

#[test]
fn test_nested_json_serialization() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "UserWithAddress").unwrap();
    let schema = (info.schema_fn)();
    let json = schema.to_json_value();
    let addr = &json["properties"]["address"];
    assert_eq!(addr["$ref"], "#/components/schemas/Address");
    let tags = &json["properties"]["tags"];
    assert_eq!(tags["type"], "array");
    assert_eq!(tags["items"]["type"], "string");
    let nick = &json["properties"]["nickname"];
    assert!(nick.get("anyOf").is_some(), "nullable should use anyOf");
}

// ---- Route Registration Tests ----

struct MockDb;

#[get("/test/{id}")]
async fn test_get_route(id: i64, db: Dep<MockDb>) -> TestUser {
    TestUser { id, name: "test".into() }
}

#[test]
fn test_route_info_registered() {
    let found = inventory::iter::<hayai::RouteInfo>()
        .find(|r| r.handler_name == "test_get_route");
    assert!(found.is_some(), "test_get_route should be registered");
    let info = found.unwrap();
    assert_eq!(info.path, "/test/{id}");
    assert_eq!(info.method, "GET");
    assert_eq!(info.response_type_name, "TestUser");
    assert_eq!(info.parameters.len(), 1);
    assert_eq!(info.parameters[0].name, "id");
}

// ---- App Builder Tests ----

#[test]
fn test_app_builder() {
    let _app = HayaiApp::new()
        .title("Test API")
        .version("0.1.0")
        .dep(MockDb);
    // If we got here, builder works
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
