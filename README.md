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
