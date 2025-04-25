use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
}; // Web framework
use serde::{Deserialize, Serialize}; // Serialization and deserialization
use sqlx::{FromRow, SqlitePool, sqlite::SqlitePoolOptions}; // Database interaction
use std::{net::SocketAddr, time::Duration};
use thiserror::Error; // Error handling
use tokio::signal; // Async runtime
use tower::ServiceBuilder; // HTTP server
use tower_http::{
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
}; // Middleware, CORS, and tracing
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi; // Automatic OpenAPI documentation

// === Domain models ===

// User database model
#[derive(Serialize, Deserialize, FromRow, ToSchema)]
struct User {
    id: i32,
    name: String,
}

// User DTO (Data Transfer Object) model
#[derive(Deserialize, ToSchema)]
struct NewUser {
    name: String,
}

// === Errors ===

// Exhaustive enum of all possible errors
#[derive(Debug, Error)]
enum AppError {
    #[error("DB: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Not found")]
    NotFound,
    #[error("Validation: {0}")]
    Validation(String),
    #[error("Startup: {0}")]
    Startup(String),
}

// Implement the IntoResponse trait for the AppError enum to convert it into a proper HTTP response
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = serde_json::json!({ "error": self.to_string() });
        (status, Json(body)).into_response()
    }
}

// === API handlers ===

#[utoipa::path(
    get,
    path = "/users",
    tag = "User Service",
    responses((status = 200, body = [User])),
    description = "Get all users"
)]
async fn get_users(State(pool): State<SqlitePool>) -> Result<Json<Vec<User>>, AppError> {
    let users = sqlx::query_as::<_, User>("SELECT * FROM users")
        .fetch_all(&pool)
        .await?;
    Ok(Json(users))
}

#[utoipa::path(
    get,
    path = "/users/{id}",
    tag = "User Service",
    params(("id" = i32, Path)),
    responses((status = 200, body = User), (status = 404)),
    description = "Get a user by ID"
)]
async fn get_user(
    Path(id): Path<i32>,
    State(pool): State<SqlitePool>,
) -> Result<Json<User>, AppError> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

#[utoipa::path(
    post,
    path = "/users",
    tag = "User Service",
    request_body = NewUser,
    responses((status = 201, body = User)),
    description = "Create a new user"
)]
async fn create_user(
    State(pool): State<SqlitePool>,
    Json(new): Json<NewUser>,
) -> Result<(StatusCode, Json<User>), AppError> {
    if new.name.trim().is_empty() {
        return Err(AppError::Validation("Name must not be empty".into()));
    }

    // Be sure to immediately return the id of the new user
    // to make it easier to use in the frontend client or API client
    let user = sqlx::query_as::<_, User>("INSERT INTO users (name) VALUES (?) RETURNING id, name")
        .bind(new.name)
        .fetch_one(&pool)
        .await?;

    Ok((StatusCode::CREATED, Json(user)))
}

// Health check route
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health Check",
    responses((status = 200, description = "Health check endpoint")),
    description = "Health check endpoint, useful for monitoring, uptime tools, and Kubernetes"
)]
async fn health() -> &'static str {
    "ok"
}

// Fallback for unknown routes
async fn fallback() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "Route not found" })),
    )
}

// === OpenAPI ===

const DESCRIPTION: &str = r#"
# Rust + Axum + SQLx + Utoipa Minimal REST API

A lightweight REST API for managing users, built with:

- [Axum](https://github.com/tokio-rs/axum) for the async web framework
- [SQLx](https://github.com/launchbadge/sqlx) for compile-time safe SQL queries
- [Utoipa](utoipa.github.io) for automatic OpenAPI documentation

### Features

- **Fast and safe**: Built in async Rust, combining performance with safety guarantees
    - Very resource efficient
- **Type-safe SQL**: All queries are validated at compile time with `sqlx`
- **Self-documenting API**: OpenAPI docs are generated directly from the route definitions using `utoipa`
- **Developer-friendly**: Simple `cargo watch -x check -x test -x run` dev cycle
- **Health check endpoint**: Easily monitored with Prometheus or external uptime tools
- **Basic endpoint tests** included — easy to extend to full integration tests
- **Deployable to AWS Lambda** in just a few lines of code
- Could also be run on a small EC2 instance or ECS container
    - On premises, use Docker to run the app in a container on local hardware
- **Low cost on AWS**:
    - ~$1.20 per **million requests** using Lambda + API Gateway
- **Clean architecture**:
    - Separation of routes, state, and error handling makes it Lambda- and container-friendly

###  What’s Next

- [ ] Add full integration tests for DB + API behavior using Schemathesis or similar against the public API and docs
- [ ] Set up a CI/CD pipeline for building, testing, and deploying the app
"#;

#[derive(OpenApi)]
#[openapi(
    paths(create_user, get_users, get_user, health),
    components(schemas(User, NewUser)),
    info(
        title = "User API",
        version = "0.1.0",
        description = DESCRIPTION,
    )
)]
struct ApiDoc;

// === Database Initialization ===
async fn initialize_database() -> Result<SqlitePool, AppError> {
    // Using an in-memory SQLite database for simplicity
    // In a production app you would use a persistent database such as PostgreSQL, AWS RDS, GCP Cloud SQL, etc.
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .map_err(|e| AppError::Startup(e.to_string()))?;

    // Create the schema
    // In production, we would use migrations
    for sql in [
        "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        "INSERT INTO users (name) VALUES ('Alice'), ('Bob')",
    ] {
        sqlx::query(sql).execute(&pool).await?;
    }

    Ok(pool)
}

// === Router Setup ===
fn build_router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/users", get(get_users).post(create_user))
        .route("/users/{id}", get(get_user))
        .route("/health", get(health))
        .fallback(fallback)
        .with_state(pool)
        .merge(SwaggerUi::new("/docs").url("/api-doc/openapi.json", ApiDoc::openapi()))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::new().allow_origin(Any))
                .layer(TimeoutLayer::new(Duration::from_secs(10))),
        )
}

// === Main Entrypoint ===
#[tokio::main]
async fn main() -> Result<(), AppError> {
    let pool = initialize_database().await?;
    let app = build_router(pool);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Docs available at http://{}/docs", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Startup(e.to_string()))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            signal::ctrl_c().await.ok();
            println!("Shutting down gracefully...");
        })
        .await
        .map_err(|e| AppError::Startup(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for `oneshot`

    async fn setup_test_app() -> Router {
        // Use the `initialize_database` function to set up an in-memory database for testing
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Create the schema for testing
        sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();

        // Seed initial data
        sqlx::query("INSERT INTO users (name) VALUES ('Alice'), ('Bob')")
            .execute(&pool)
            .await
            .unwrap();

        // Use the `build_router` function to create the app
        build_router(pool)
    }

    #[tokio::test]
    async fn test_health_check() {
        // Arrange
        let pool = SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .expect("Failed to create SQLite pool");
        let app = build_router(pool);

        // Act
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn test_get_users() {
        // Arrange
        let app = setup_test_app().await;

        // Act
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let users: Vec<User> = serde_json::from_slice(&body).unwrap();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].name, "Alice");
        assert_eq!(users[1].name, "Bob");
    }

    #[tokio::test]
    async fn test_get_user_not_found() {
        // Arrange
        let app = setup_test_app().await;

        // Act
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users/999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let error_message: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(error_message["error"], "Not found");
    }

    #[tokio::test]
    async fn test_create_user() {
        // Arrange
        let app = setup_test_app().await;

        // Act
        let new_user = serde_json::json!({ "name": "Charlie" });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/users")
                    .header("Content-Type", "application/json")
                    .body(Body::from(new_user.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let created_user: User = serde_json::from_slice(&body).unwrap();
        assert_eq!(created_user.name, "Charlie");
    }
}
