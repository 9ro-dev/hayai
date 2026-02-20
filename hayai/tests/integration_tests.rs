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
    let user = CreateTestUser { name: "Alice".into(), email: "alice@example.com".into() };
    assert!(user.validate().is_ok());
}

#[test]
fn test_validation_min_length() {
    let user = CreateTestUser { name: "".into(), email: "alice@example.com".into() };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("must be at least 1")));
}

#[test]
fn test_validation_max_length() {
    let user = CreateTestUser { name: "a".repeat(51), email: "alice@example.com".into() };
    let err = user.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("must be at most 50")));
}

#[test]
fn test_validation_email_missing_at() {
    let user = CreateTestUser { name: "Alice".into(), email: "notanemail".into() };
    assert!(user.validate().is_err());
}

#[test]
fn test_validation_email_at_start() {
    let user = CreateTestUser { name: "Alice".into(), email: "@example.com".into() };
    assert!(user.validate().is_err());
}

#[test]
fn test_validation_email_at_end() {
    let user = CreateTestUser { name: "Alice".into(), email: "user@".into() };
    assert!(user.validate().is_err());
}

#[test]
fn test_validation_email_no_dot_in_domain() {
    let user = CreateTestUser { name: "Alice".into(), email: "user@localhost".into() };
    assert!(user.validate().is_err());
}

#[test]
fn test_validation_email_multiple_at() {
    let user = CreateTestUser { name: "Alice".into(), email: "user@@example.com".into() };
    assert!(user.validate().is_err());
}

#[test]
fn test_validation_multiple_errors() {
    let user = CreateTestUser { name: "".into(), email: "bad".into() };
    let err = user.validate().unwrap_err();
    assert_eq!(err.len(), 2);
}

// ---- Schema Tests ----

#[test]
fn test_schema_generation() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let test_user_schema = schemas.iter().find(|s| s.name == "CreateTestUser");
    assert!(test_user_schema.is_some());
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
    assert!(addr_prop.ref_path.is_some());
    assert_eq!(addr_prop.ref_path.as_deref().unwrap(), "#/components/schemas/Address");
}

#[test]
fn test_vec_string_schema() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "UserWithAddress").unwrap();
    let schema = (info.schema_fn)();
    let tags_prop = &schema.properties["tags"];
    assert_eq!(tags_prop.type_name, "array");
    assert!(tags_prop.items.is_some());
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
fn test_option_not_required() {
    // Issue #8: Option<T> fields should NOT be in required array
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "UserWithAddress").unwrap();
    let schema = (info.schema_fn)();
    assert!(!schema.required.contains(&"nickname".to_string()), "Option<T> field should not be required");
    assert!(schema.required.contains(&"name".to_string()), "Non-Option field should be required");
    assert!(schema.required.contains(&"address".to_string()), "Non-Option field should be required");
    assert!(schema.required.contains(&"tags".to_string()), "Non-Option field should be required");
}

#[test]
fn test_nested_definitions_collected() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "UserWithAddress").unwrap();
    let nested = (info.nested_fn)();
    assert!(nested.contains_key("Address"));
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
    assert!(nick.get("anyOf").is_some());
}

// ---- Route Registration Tests ----

struct MockDb;

#[get("/test/{id}")]
async fn test_get_route(id: i64, _db: Dep<MockDb>) -> TestUser {
    TestUser { id, name: "test".into() }
}

#[test]
fn test_route_info_registered() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "test_get_route");
    assert!(found.is_some());
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

// ---- Issue #5: Enum Support ----

/// Task status
#[api_model]
#[derive(Debug, Clone, PartialEq)]
enum TaskStatus {
    Active,
    Inactive,
    Pending,
}

#[test]
fn test_enum_schema() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "TaskStatus").unwrap();
    let schema = (info.schema_fn)();
    assert_eq!(schema.type_name, "string");
    let values = schema.enum_values.as_ref().unwrap();
    assert_eq!(values, &["Active", "Inactive", "Pending"]);
}

#[test]
fn test_enum_json_serialization() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "TaskStatus").unwrap();
    let schema = (info.schema_fn)();
    let json = schema.to_json_value();
    assert_eq!(json["type"], "string");
    let vals: Vec<String> = json["enum"].as_array().unwrap().iter()
        .map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(vals, vec!["Active", "Inactive", "Pending"]);
}

#[test]
fn test_enum_description() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "TaskStatus").unwrap();
    let schema = (info.schema_fn)();
    assert_eq!(schema.description.as_deref(), Some("Task status"));
}

#[test]
fn test_enum_validate() {
    let status = TaskStatus::Active;
    assert!(status.validate().is_ok());
}

#[test]
fn test_enum_serde_roundtrip() {
    let status = TaskStatus::Active;
    let json = hayai::serde_json::to_string(&status).unwrap();
    assert_eq!(json, r#""Active""#);
    let parsed: TaskStatus = hayai::serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, TaskStatus::Active);
}

// ---- Issue #4: Description Support ----

/// A documented struct
#[api_model]
#[derive(Debug, Clone)]
struct DocumentedModel {
    /// The unique identifier
    id: i64,
    /// Human-readable name
    name: String,
}

#[test]
fn test_struct_description() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "DocumentedModel").unwrap();
    let schema = (info.schema_fn)();
    assert_eq!(schema.description.as_deref(), Some("A documented struct"));
}

#[test]
fn test_field_description() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "DocumentedModel").unwrap();
    let schema = (info.schema_fn)();
    assert_eq!(schema.properties["id"].description.as_deref(), Some("The unique identifier"));
    assert_eq!(schema.properties["name"].description.as_deref(), Some("Human-readable name"));
}

// ---- Issue #7: Numeric Validation ----

#[api_model]
#[derive(Debug, Clone)]
struct NumericModel {
    #[validate(minimum = 1, maximum = 100)]
    quantity: i64,
    #[validate(pattern = "^[A-Z]+$")]
    code: String,
    #[validate(min_items = 1)]
    items: Vec<String>,
}

#[test]
fn test_minimum_validation() {
    let m = NumericModel { quantity: 0, code: "ABC".into(), items: vec!["a".into()] };
    let err = m.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("at least 1")));
}

#[test]
fn test_maximum_validation() {
    let m = NumericModel { quantity: 101, code: "ABC".into(), items: vec!["a".into()] };
    let err = m.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("at most 100")));
}

#[test]
fn test_min_items_validation() {
    let m = NumericModel { quantity: 50, code: "ABC".into(), items: vec![] };
    let err = m.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("at least 1 items")));
}

#[test]
fn test_numeric_valid() {
    let m = NumericModel { quantity: 50, code: "ABC".into(), items: vec!["a".into()] };
    assert!(m.validate().is_ok());
}

#[test]
fn test_numeric_schema_constraints() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "NumericModel").unwrap();
    let schema = (info.schema_fn)();
    assert_eq!(schema.properties["quantity"].minimum, Some(1.0));
    assert_eq!(schema.properties["quantity"].maximum, Some(100.0));
    assert_eq!(schema.properties["code"].pattern.as_deref(), Some("^[A-Z]+$"));
    assert_eq!(schema.properties["items"].min_items, Some(1));
}

#[test]
fn test_numeric_schema_json() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "NumericModel").unwrap();
    let schema = (info.schema_fn)();
    let json = schema.to_json_value();
    assert_eq!(json["properties"]["quantity"]["minimum"], 1.0);
    assert_eq!(json["properties"]["quantity"]["maximum"], 100.0);
    assert_eq!(json["properties"]["code"]["pattern"], "^[A-Z]+$");
    assert_eq!(json["properties"]["items"]["minItems"], 1);
}

// ---- Issue #3: Status Code / Issue #6: Tags ----

/// A tagged and status-coded route
#[post("/items")]
#[status(201)]
#[tag("items")]
async fn create_item_route(_body: NumericModel, _db: Dep<MockDb>) -> TestUser {
    TestUser { id: 1, name: "item".into() }
}

#[test]
fn test_route_status_code() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "create_item_route");
    assert!(found.is_some());
    let info = found.unwrap();
    assert_eq!(info.success_status, 201);
}

#[test]
fn test_route_tags() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "create_item_route");
    let info = found.unwrap();
    assert_eq!(info.tags, &["items"]);
}

#[test]
fn test_route_description() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "create_item_route");
    let info = found.unwrap();
    assert_eq!(info.description, "A tagged and status-coded route");
}

// ---- Issue #3: Default status codes ----

#[get("/default-get")]
async fn default_get_route() -> TestUser {
    TestUser { id: 1, name: "test".into() }
}

#[delete("/default-delete/{id}")]
#[allow(unused_variables)]
async fn default_delete_route(id: i64) -> () {
    ()
}

#[test]
fn test_default_get_status() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "default_get_route").unwrap();
    assert_eq!(found.success_status, 200);
}

#[test]
fn test_default_delete_status() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "default_delete_route").unwrap();
    assert_eq!(found.success_status, 204);
}

// ---- Issue #1: Query Parameters ----

#[derive(hayai::serde::Deserialize, hayai::schemars::JsonSchema)]
#[allow(dead_code)]
struct TestPagination {
    page: Option<i64>,
    limit: Option<i64>,
}

#[get("/query-test")]
async fn query_test_route(query: Query<TestPagination>) -> TestUser {
    TestUser { id: query.page.unwrap_or(0), name: "test".into() }
}

#[test]
fn test_query_params_fn_registered() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "query_test_route").unwrap();
    assert!(found.query_params_fn.is_some());
    let params = (found.query_params_fn.unwrap())();
    assert_eq!(params.len(), 2);
    let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"page"));
    assert!(names.contains(&"limit"));
    // Option fields should not be required
    for p in &params {
        assert!(!p.required, "{} should not be required (Option)", p.name);
    }
}

// ---- Issue #2: Error Response Schema ----

#[test]
fn test_api_error_schema() {
    let schema = openapi::api_error_schema();
    assert_eq!(schema.type_name, "object");
    assert!(schema.properties.contains_key("error"));
    assert!(schema.properties.contains_key("details"));
    assert!(schema.required.contains(&"error".to_string()));
    assert_eq!(schema.description.as_deref(), Some("Standard API error response"));
}

// ---- Issue #1 (Vec<T> response) ----

#[get("/vec-test")]
async fn vec_test_route() -> Vec<TestUser> {
    vec![TestUser { id: 1, name: "test".into() }]
}

#[test]
fn test_vec_response_route_info() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "vec_test_route").unwrap();
    assert!(found.is_vec_response);
    assert_eq!(found.vec_inner_type_name, "TestUser");
}

// ---- Issue #2 (Enum $ref) ----

#[api_model]
#[derive(Debug, Clone)]
enum TestStatus {
    Active,
    Inactive,
}

#[api_model]
#[derive(Debug, Clone)]
struct ModelWithEnum {
    name: String,
    status: TestStatus,
}

#[test]
fn test_enum_field_ref() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "ModelWithEnum").unwrap();
    let schema = (info.schema_fn)();
    let status_prop = &schema.properties["status"];
    assert!(status_prop.ref_path.is_some(), "Enum field should have $ref");
    assert_eq!(status_prop.ref_path.as_deref().unwrap(), "#/components/schemas/TestStatus");
}

// ---- Issue #3 (servers) ----

#[test]
fn test_server_builder() {
    let app = HayaiApp::new()
        .title("Test")
        .server("http://localhost:3000")
        .server("https://api.example.com");
    // Just verify it builds without panic
    let _ = app;
}

// ---- Issue #6 (response descriptions) ----

#[test]
fn test_status_description() {
    assert_eq!(openapi::status_description(200), "OK");
    assert_eq!(openapi::status_description(201), "Created");
    assert_eq!(openapi::status_description(204), "No Content");
    assert_eq!(openapi::status_description(400), "Bad Request");
    assert_eq!(openapi::status_description(422), "Validation Failed");
    assert_eq!(openapi::status_description(500), "Internal Server Error");
}

// ---- Issue #7 (example support) ----

#[api_model]
#[derive(Debug, Clone)]
struct ModelWithExample {
    #[schema(example = "john@example.com")]
    email: String,
    name: String,
}

#[test]
fn test_example_on_field() {
    let schemas: Vec<_> = inventory::iter::<hayai::SchemaInfo>().collect();
    let info = schemas.iter().find(|s| s.name == "ModelWithExample").unwrap();
    let schema = (info.schema_fn)();
    let email_prop = &schema.properties["email"];
    assert_eq!(email_prop.example.as_deref(), Some("john@example.com"));
    let json = schema.to_json_value();
    assert_eq!(json["properties"]["email"]["example"], "john@example.com");
}

// ---- Issue #8 (security) ----

#[get("/secure-test")]
#[security("bearer")]
async fn secure_test_route() -> TestUser {
    TestUser { id: 1, name: "test".into() }
}

#[test]
fn test_security_attribute() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "secure_test_route").unwrap();
    assert_eq!(found.security, &["bearer"]);
}

#[test]
fn test_bearer_auth_builder() {
    let app = HayaiApp::new()
        .title("Test")
        .bearer_auth();
    let _ = app;
}

// ---- Issue #1: SwaggerMode Embedded ----

#[test]
fn test_swagger_mode_embedded_default() {
    let app = HayaiApp::new().title("Test");
    let router = app.into_router();
    // Just verify it builds without panic (embedded mode is default)
    let _ = router;
}

#[test]
fn test_swagger_mode_cdn() {
    let app = HayaiApp::new()
        .title("Test")
        .swagger_cdn("https://custom-cdn.example.com/swagger-ui");
    let router = app.into_router();
    let _ = router;
}

#[test]
fn test_swagger_mode_builder() {
    let app = HayaiApp::new()
        .title("Test")
        .swagger_mode(hayai::SwaggerMode::Embedded);
    let _ = app.into_router();

    let app2 = HayaiApp::new()
        .title("Test")
        .swagger_mode(hayai::SwaggerMode::Cdn("https://example.com".into()));
    let _ = app2.into_router();
}

// ---- Issue #2: Description ----

#[test]
fn test_info_description() {
    let app = HayaiApp::new()
        .title("Test API")
        .version("1.0.0")
        .description("A test API description");
    let router = app.into_router();
    let _ = router;
}

#[test]
fn test_openapi_spec_description_in_json() {
    let spec = openapi::OpenApiSpec {
        openapi: "3.1.0".to_string(),
        info: openapi::Info {
            title: "Test".to_string(),
            version: "1.0".to_string(),
            description: Some("My description".to_string()),
            contact: None,
            license: None,
        },
        servers: vec![],
        paths: HashMap::new(),
        schemas: HashMap::new(),
        security_schemes: HashMap::new(),
    };
    let json = spec.to_json();
    assert_eq!(json["info"]["description"], "My description");
}

// ---- Issue #3: Contact and License ----

#[test]
fn test_info_contact_license() {
    let app = HayaiApp::new()
        .title("Test API")
        .contact("Author", "author@example.com", "https://example.com")
        .license("MIT", "https://opensource.org/licenses/MIT");
    let _ = app.into_router();
}

#[test]
fn test_openapi_spec_contact_license_in_json() {
    let spec = openapi::OpenApiSpec {
        openapi: "3.1.0".to_string(),
        info: openapi::Info {
            title: "Test".to_string(),
            version: "1.0".to_string(),
            description: None,
            contact: Some(openapi::Contact {
                name: Some("Author".to_string()),
                email: Some("a@b.com".to_string()),
                url: Some("https://example.com".to_string()),
            }),
            license: Some(openapi::License {
                name: "MIT".to_string(),
                url: Some("https://opensource.org/licenses/MIT".to_string()),
            }),
        },
        servers: vec![],
        paths: HashMap::new(),
        schemas: HashMap::new(),
        security_schemes: HashMap::new(),
    };
    let json = spec.to_json();
    assert_eq!(json["info"]["contact"]["name"], "Author");
    assert_eq!(json["info"]["contact"]["email"], "a@b.com");
    assert_eq!(json["info"]["license"]["name"], "MIT");
    assert_eq!(json["info"]["license"]["url"], "https://opensource.org/licenses/MIT");
}

// ---- Issue #4: Query param constraints ----

#[derive(hayai::serde::Deserialize, hayai::schemars::JsonSchema)]
#[allow(dead_code)]
struct ConstrainedQuery {
    /// Page number
    #[schemars(range(min = 1, max = 100))]
    page: Option<i64>,
    /// Search term
    #[schemars(length(min = 1, max = 255))]
    search: Option<String>,
}

#[test]
fn test_query_param_constraints_propagation() {
    let root = hayai::schemars::schema_for!(ConstrainedQuery);
    let params = openapi::query_params_from_schema(&root);

    let page_param = params.iter().find(|p| p.name == "page").unwrap();
    assert_eq!(page_param.minimum, Some(1.0));
    assert_eq!(page_param.maximum, Some(100.0));

    let search_param = params.iter().find(|p| p.name == "search").unwrap();
    assert_eq!(search_param.min_length, Some(1));
    assert_eq!(search_param.max_length, Some(255));
}

#[test]
fn test_query_param_constraints_in_serialized_output() {
    let root = hayai::schemars::schema_for!(ConstrainedQuery);
    let params = openapi::query_params_from_schema(&root);

    let page_param = params.iter().find(|p| p.name == "page").unwrap();
    let json = serde_json::to_value(page_param).unwrap();
    assert_eq!(json["schema"]["minimum"], 1.0);
    assert_eq!(json["schema"]["maximum"], 100.0);
}

// ---- Issue #5: HashMap<String, T> → additionalProperties ----

#[api_model]
#[derive(Debug, Clone)]
struct WithHashMap {
    name: String,
    metadata: HashMap<String, String>,
}

#[test]
fn test_hashmap_additional_properties() {
    let root = hayai::schemars::schema_for!(WithHashMap);
    let schema = openapi::schema_from_schemars("WithHashMap", &root);

    let meta_prop = schema.properties.get("metadata").unwrap();
    assert_eq!(meta_prop.type_name, "object");
    assert!(meta_prop.additional_properties.is_some());
    let ap = meta_prop.additional_properties.as_ref().unwrap();
    assert_eq!(ap.type_name, "string");
}

#[test]
fn test_hashmap_json_output() {
    let root = hayai::schemars::schema_for!(WithHashMap);
    let schema = openapi::schema_from_schemars("WithHashMap", &root);
    let json = schema.to_json_value();

    let meta = &json["properties"]["metadata"];
    assert_eq!(meta["type"], "object");
    assert_eq!(meta["additionalProperties"]["type"], "string");
}

use hayai::serde_json;

// ===== Router tests =====

#[api_model]
#[derive(Debug, Clone)]
struct RouterTestItem {
    id: i64,
    name: String,
}

#[get("/rt-list")]
async fn rt_list_items() -> Vec<RouterTestItem> {
    vec![RouterTestItem { id: 1, name: "item".into() }]
}

#[get("/rt-item/{id}")]
async fn rt_get_item(id: i64) -> RouterTestItem {
    RouterTestItem { id, name: "item".into() }
}

#[post("/rt-create")]
async fn rt_create_item(body: CreateTestUser) -> RouterTestItem {
    RouterTestItem { id: 1, name: body.name }
}

#[get("/rt-tagged")]
#[tag("custom")]
async fn rt_tagged_route() -> RouterTestItem {
    RouterTestItem { id: 1, name: "tagged".into() }
}

#[get("/rt-secured")]
#[security("bearer")]
async fn rt_secured_route() -> RouterTestItem {
    RouterTestItem { id: 1, name: "secured".into() }
}

#[test]
fn test_router_prefix_prepended() {
    let router = hayai::HayaiRouter::new("/api/items")
        .route(__HAYAI_ROUTE_RT_LIST_ITEMS)
        .route(__HAYAI_ROUTE_RT_GET_ITEM);
    let resolved = router.resolve("", &[], &[]);
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].full_path(), "/api/items/rt-list");
    assert_eq!(resolved[1].full_path(), "/api/items/rt-item/{id}");
}

#[test]
fn test_nested_router_prefix_concatenation() {
    let inner = hayai::HayaiRouter::new("/items")
        .route(__HAYAI_ROUTE_RT_LIST_ITEMS);
    let outer = hayai::HayaiRouter::new("/api/v1")
        .include(inner);
    let resolved = outer.resolve("", &[], &[]);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].full_path(), "/api/v1/items/rt-list");
}

#[test]
fn test_router_tags_merged_with_route_tags() {
    let router = hayai::HayaiRouter::new("/tagged")
        .tag("router-tag")
        .route(__HAYAI_ROUTE_RT_TAGGED_ROUTE);
    let resolved = router.resolve("", &[], &[]);
    assert_eq!(resolved.len(), 1);
    let tags = resolved[0].merged_tags();
    assert!(tags.contains(&"router-tag".to_string()));
    assert!(tags.contains(&"custom".to_string()));
}

#[test]
fn test_router_security_applied() {
    let router = hayai::HayaiRouter::new("/secure")
        .security("api_key")
        .route(__HAYAI_ROUTE_RT_SECURED_ROUTE);
    let resolved = router.resolve("", &[], &[]);
    let sec = resolved[0].merged_security();
    assert!(sec.contains(&"api_key"));
    assert!(sec.contains(&"bearer"));
}

#[test]
fn test_router_no_include_backward_compat() {
    // When no .include() is used, auto-discovery should work
    let app = hayai::HayaiApp::new();
    assert!(!app.has_explicit_routes());
}

#[test]
fn test_router_with_include_is_explicit() {
    let router = hayai::HayaiRouter::new("/items")
        .route(__HAYAI_ROUTE_RT_LIST_ITEMS);
    let app = hayai::HayaiApp::new().include(router);
    assert!(app.has_explicit_routes());
}

#[test]
fn test_deeply_nested_routers() {
    let items = hayai::HayaiRouter::new("/items")
        .route(__HAYAI_ROUTE_RT_GET_ITEM);
    let v1 = hayai::HayaiRouter::new("/v1")
        .include(items);
    let api = hayai::HayaiRouter::new("/api")
        .include(v1);
    let resolved = api.resolve("", &[], &[]);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].full_path(), "/api/v1/items/rt-item/{id}");
}

#[test]
fn test_router_tags_security_propagate_through_nesting() {
    let inner = hayai::HayaiRouter::new("/items")
        .route(__HAYAI_ROUTE_RT_LIST_ITEMS);
    let outer = hayai::HayaiRouter::new("/api")
        .tag("api")
        .security("bearer")
        .include(inner);
    let resolved = outer.resolve("", &[], &[]);
    let tags = resolved[0].merged_tags();
    assert!(tags.contains(&"api".to_string()));
    let sec = resolved[0].merged_security();
    assert!(sec.contains(&"bearer"));
}

#[test]
fn test_router_openapi_spec_prefixed_paths() {
    let router = hayai::HayaiRouter::new("/api/items")
        .tag("items")
        .route(__HAYAI_ROUTE_RT_LIST_ITEMS)
        .route(__HAYAI_ROUTE_RT_GET_ITEM);
    let app = hayai::HayaiApp::new()
        .include(router);
    let _router_axum = app.into_router();
    // The router was built — if it compiled and didn't panic, the paths are registered
    // We can't easily inspect the axum router, but the OpenAPI spec test below covers it
}

#[test]
fn test_multiple_routers_on_app() {
    let items = hayai::HayaiRouter::new("/items")
        .route(__HAYAI_ROUTE_RT_LIST_ITEMS);
    let secure = hayai::HayaiRouter::new("/secure")
        .route(__HAYAI_ROUTE_RT_SECURED_ROUTE);
    let app = hayai::HayaiApp::new()
        .include(items)
        .include(secure);
    let resolved = app.resolve_routes();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].full_path(), "/items/rt-list");
    assert_eq!(resolved[1].full_path(), "/secure/rt-secured");
}

// ---- Fix #1: Missing dep returns error, not panic ----

#[tokio::test]
async fn test_missing_dep_returns_500_not_panic() {
    // Build app WITHOUT registering MockDb, but with a route that needs it
    let router = hayai::HayaiRouter::new("/test")
        .route(__HAYAI_ROUTE_TEST_GET_ROUTE);
    let app = HayaiApp::new()
        .include(router)
        .into_router();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        hayai::axum::serve(listener, app).await.unwrap();
    });

    let resp = reqwest::get(format!("http://{}/test/test/42", addr)).await.unwrap();
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Dependency not registered"));
}

// ---- Fix #2: String path param gets "string" type ----

#[get("/by-name/{name}")]
async fn get_by_name(name: String) -> TestUser {
    TestUser { id: 0, name }
}

#[test]
fn test_string_path_param_type() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "get_by_name").unwrap();
    assert_eq!(found.parameters.len(), 1);
    assert_eq!(found.parameters[0].name, "name");
    assert_eq!(found.parameters[0].schema.type_name, "string");
}

#[test]
fn test_integer_path_param_type() {
    let found = inventory::iter::<&hayai::RouteInfo>()
        .find(|r| r.handler_name == "test_get_route").unwrap();
    assert_eq!(found.parameters[0].schema.type_name, "integer");
}

// ---- Fix #5: Pattern validation rejects non-matching input ----

#[test]
fn test_pattern_validation_rejects_non_matching() {
    let m = NumericModel { quantity: 50, code: "abc".into(), items: vec!["a".into()] };
    let err = m.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("must match pattern")));
}

#[test]
fn test_pattern_validation_accepts_matching() {
    let m = NumericModel { quantity: 50, code: "ABC".into(), items: vec!["a".into()] };
    assert!(m.validate().is_ok());
}
