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

/// Get a user by ID
#[get("/users/{id}")]
#[tag("users")]
#[security("bearer")]
async fn get_user(id: i64, db: Dep<Database>) -> User {
    db.get_user(id).await.unwrap()
}

/// List all users with pagination
#[get("/users")]
#[tag("users")]
#[security("bearer")]
async fn list_users(query: Query<Pagination>, db: Dep<Database>) -> Vec<User> {
    db.list_users(query.page, query.limit).await
}

/// Create a new user
#[post("/users")]
#[tag("users")]
#[security("bearer")]
async fn create_user(body: CreateUser, db: Dep<Database>) -> User {
    db.create_user(&body).await
}

/// Delete a user by ID
#[delete("/users/{id}")]
#[tag("users")]
#[security("bearer")]
async fn delete_user(id: i64, db: Dep<Database>) -> () {
    db.delete_user(id).await
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
        .serve("0.0.0.0:3001")
        .await;
}
