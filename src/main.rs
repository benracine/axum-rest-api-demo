use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, sqlite::SqlitePoolOptions};
use std::net::SocketAddr;
use thiserror::Error;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Serialize, Deserialize, FromRow, ToSchema)]
struct User {
    id: i32,
    name: String,
}

#[derive(Deserialize, ToSchema)]
struct NewUser {
    name: String,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("DB: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Not found")]
    NotFound,
    #[error("Startup: {0}")]
    Startup(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = if matches!(self, AppError::NotFound) {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, self.to_string()).into_response()
    }
}

#[utoipa::path(
    get,
    path = "/users",
    tag = "Users",
    responses((status = 200, body = User), (status = 404))
)]
async fn get_users(State(pool): State<SqlitePool>) -> Result<Json<Vec<User>>, AppError> {
    let users = sqlx::query_as::<_, User>("SELECT * FROM users")
        .fetch_all(&pool)
        .await?;

    if users.is_empty() {
        Err(AppError::NotFound)
    } else {
        Ok(Json(users))
    }
}

#[utoipa::path(
    get,
    path = "/users/{id}",
    tag = "Users",
    params(("id" = i32, Path)),
    responses((status = 200, body = User), (status = 404))
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
    tag = "Users",
    request_body = NewUser,
    responses((status = 201, body = User))
)]
async fn create_user(
    State(pool): State<SqlitePool>,
    Json(new): Json<NewUser>,
) -> Result<(StatusCode, Json<User>), AppError> {
    let user = sqlx::query_as::<_, User>("INSERT INTO users (name) VALUES (?) RETURNING id, name")
        .bind(new.name)
        .fetch_one(&pool)
        .await?;

    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(OpenApi)]
#[openapi(
    paths(create_user, get_users, get_user),
    components(schemas(User, NewUser)),
    info(
        title = "User Management API",
        version = "0.1.0",
        description = "Simple API for managing users",
        contact(name = "Ben Racine", email = "benracinedev@gmail.com",)
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .map_err(|e| AppError::Startup(e.to_string()))?;

    for sql in [
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        "INSERT INTO users (name) VALUES ('Alice'), ('Bob')",
    ] {
        sqlx::query(sql).execute(&pool).await?;
    }

    let app = Router::new()
        .route("/users", post(create_user))
        .route("/users", get(get_users))
        .route("/users/{id}", get(get_user))
        .with_state(pool)
        .merge(SwaggerUi::new("/docs").url("/api-doc/openapi.json", ApiDoc::openapi()));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Docs: http://{}/docs", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Startup(e.to_string()))?;

    axum::serve(listener, app).await.unwrap();

    Ok(())
}
