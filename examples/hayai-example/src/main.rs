use hayai::prelude::*;
use serde::{Serialize, Deserialize};
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ApiModel)]
struct User {
    id: i64,
    name: String,
    email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ApiModel)]
struct CreateUser {
    #[validate(min_length = 1, max_length = 100)]
    name: String,
    #[validate(email)]
    email: String,
}

struct Database;
impl Database {
    async fn get_user(&self, id: i64) -> Option<User> {
        Some(User {
            id,
            name: "Alice".into(),
            email: "alice@example.com".into(),
        })
    }

    async fn create_user(&self, input: &CreateUser) -> User {
        User {
            id: 1,
            name: input.name.clone(),
            email: input.email.clone(),
        }
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

#[tokio::main]
async fn main() {
    HayaiApp::new()
        .dep(Database)
        .route(get_user)
        .route(create_user)
        .serve("0.0.0.0:3000")
        .await;
}
