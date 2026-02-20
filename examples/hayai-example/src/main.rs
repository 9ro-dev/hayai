use hayai::prelude::*;
use hayai::serde::Deserialize;
use hayai::schemars::JsonSchema;
use std::collections::HashMap;

/// A user in the system
#[api_model]
#[derive(Debug, Clone)]
struct User {
    id: i64,
    /// Display name
    name: String,
    /// Email address
    #[schema(example = "alice@example.com")]
    email: String,
    /// Current account status
    status: Status,
}

/// Account status
#[api_model]
#[derive(Debug, Clone)]
enum Status {
    Active,
    Inactive,
    Pending,
}

/// Request body for creating a user
#[api_model]
#[derive(Debug, Clone)]
struct CreateUser {
    #[validate(min_length = 1, max_length = 100)]
    name: String,
    #[validate(email)]
    #[schema(example = "john@example.com")]
    email: String,
}

/// An item with numeric constraints
#[api_model]
#[derive(Debug, Clone)]
struct CreateItem {
    #[validate(min_length = 1)]
    name: String,
    /// Quantity to order
    #[validate(minimum = 1, maximum = 1000)]
    quantity: i64,
    /// Product code (3 uppercase letters)
    #[validate(pattern = "^[A-Z]{3}$")]
    code: String,
    #[validate(min_items = 1)]
    tags: Vec<String>,
}

#[api_model]
#[derive(Debug, Clone)]
struct Address {
    city: String,
    country: String,
}

#[api_model]
#[derive(Debug, Clone)]
struct UserProfile {
    name: String,
    address: Address,
    tags: Vec<String>,
    nickname: Option<String>,
    /// Custom metadata key-value pairs
    metadata: HashMap<String, String>,
}

/// Query parameters for listing users
#[derive(Deserialize, JsonSchema)]
struct Pagination {
    /// Page number (1-based)
    page: Option<i64>,
    /// Number of items per page
    limit: Option<i64>,
}

struct Database;
impl Database {
    async fn get_user(&self, id: i64) -> Option<User> {
        Some(User {
            id,
            name: "Alice".into(),
            email: "alice@example.com".into(),
            status: Status::Active,
        })
    }

    async fn list_users(&self, _page: Option<i64>, _limit: Option<i64>) -> Vec<User> {
        vec![User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
            status: Status::Active,
        }]
    }

    async fn create_user(&self, input: &CreateUser) -> User {
        User {
            id: 1,
            name: input.name.clone(),
            email: input.email.clone(),
            status: Status::Pending,
        }
    }

    async fn delete_user(&self, _id: i64) {}
}

/// Custom auth validator that accepts API keys starting with "sk-"
#[derive(Clone)]
struct ApiKeyValidator;

#[async_trait::async_trait]
impl AuthValidator for ApiKeyValidator {
    type Credentials = String;

    async fn validate(&self, token: &str) -> Result<Self::Credentials, ApiError> {
        // Accept tokens starting with "sk-" for demo
        if token.starts_with("sk-") {
            Ok(token.to_string())
        } else {
            Err(ApiError::unauthorized("Invalid API key - must start with 'sk-'"))
        }
    }
}

/// Get a user by ID
#[get("/{id}")]
async fn get_user(id: i64, db: Dep<Database>) -> Result<User, ApiError> {
    db.get_user(id).await
        .ok_or_else(|| ApiError::not_found(format!("User {} not found", id)))
}

/// List all users with pagination (using State<T> instead of Dep<T>)
#[get("/")]
async fn list_users(query: Query<Pagination>, db: State<Database>) -> Vec<User> {
    db.list_users(query.page, query.limit).await
}

/// Create a new user
#[post("/")]
async fn create_user(body: CreateUser, db: Dep<Database>) -> User {
    db.create_user(&body).await
}

/// Delete a user by ID
#[delete("/{id}")]
async fn delete_user(id: i64, db: Dep<Database>) -> () {
    db.delete_user(id).await
}

/// Create a new item
#[post("/")]
async fn create_item(body: CreateItem) -> CreateItem {
    body
}

/// Protected route that requires authentication
/// Uses the ApiKeyValidator to verify the Authorization header
#[get("/protected")]
#[security("bearer")]
async fn protected_route(auth: Auth<ApiKeyValidator>) -> String {
    format!("Hello, authenticated user! Your key: {}", auth.user)
}

fn user_routes() -> HayaiRouter {
    HayaiRouter::new("/users")
        .tag("users")
        .security("bearer")
        .route(__HAYAI_ROUTE_GET_USER)
        .route(__HAYAI_ROUTE_LIST_USERS)
        .route(__HAYAI_ROUTE_CREATE_USER)
        .route(__HAYAI_ROUTE_DELETE_USER)
        .route(__HAYAI_ROUTE_PROTECTED_ROUTE)
}

fn item_routes() -> HayaiRouter {
    HayaiRouter::new("/items")
        .tag("items")
        .route(__HAYAI_ROUTE_CREATE_ITEM)
}

#[tokio::main]
async fn main() {
    HayaiApp::new()
        .title("My API")
        .version("1.0.0")
        .description("A sample API demonstrating Hayai features")
        .contact("Author", "author@example.com", "https://example.com")
        .license("MIT", "https://opensource.org/licenses/MIT")
        .server("http://localhost:3001")
        .bearer_auth()
        .dep(Database)
        .dep(ApiKeyValidator)  // Register the auth validator
        .include(user_routes())
        .include(item_routes())
        .serve("0.0.0.0:3001")
        .await;
}
