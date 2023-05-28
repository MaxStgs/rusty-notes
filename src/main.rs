use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use serde::Serialize;
use tokio_postgres::{Config, NoTls, Row};
use std::sync::Arc;
use actix_web::middleware::Logger;
use actix_service::{Service, Transform as TransformTrait};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use futures::future::{ok, Ready};
use std::task::{Context, Poll};

type ErrorMigration = Box<dyn std::error::Error + Send + Sync + 'static>;

struct AppState {
    db_pool: Arc<tokio_postgres::Client>,
}

pub struct LoggingMiddleware;

impl<S, B> TransformTrait<S> for LoggingMiddleware
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    B: actix_http::body::MessageBody,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = LoggingMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(LoggingMiddlewareService { service })
    }
}

pub struct LoggingMiddlewareService<S> {
    service: S,
}

impl<S, B> Service for LoggingMiddlewareService<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    B: actix_http::body::MessageBody,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);

        async move {
            let response = fut.await;

            if let Err(ref e) = response {
                eprintln!("Error: {:?}", e);
            }

            response
        }
    }
}

#[derive(Debug, Serialize)]
struct Note {
    id: i32,
    title: String,
    description: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn connect_db() -> Result<Arc<tokio_postgres::Client>, Box<dyn std::error::Error>> {
    let mut config = Config::new();
    config
        .host("localhost")
        .port(5432)
        .dbname("notes")
        .user("admin")
        .password("admin");

    let (client, connection) = config.connect(NoTls).await?;

    // Запускаем соединение с БД в отдельной задаче
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    Ok(Arc::new(client))
}

fn db_error_handler(error: tokio_postgres::Error) -> actix_web::Error {
    actix_web::error::InternalError::from_response(
        "Internal Server Error",
        HttpResponse::InternalServerError().json(
            serde_json::json!({ "error": error.to_string() }),
        ),
    )
    .into()
}

async fn get_notes(
    data: web::Data<AppState>,
    web::Query(query): web::Query<std::collections::HashMap<String, String>>,
) -> actix_web::Result<HttpResponse> {
    let limit = 50;
    let offset = query
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let rows = data.db_pool
        .query("SELECT * FROM notes WHERE deleted_at IS NULL LIMIT $1 OFFSET $2", &[&limit, &offset])
        .await
        .map_err(db_error_handler)?;

    let notes: Vec<Note> = rows
        .iter()
        .map(|row: &Row| Note {
            id: row.get("id"),
            title: row.get("title"),
            description: row.get("description"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();

    Ok(HttpResponse::Ok().json(notes))
}

async fn healthcheck(data: web::Data<AppState>) -> impl Responder {
    let is_db_connected = !data.db_pool.is_closed();
    if is_db_connected {
        HttpResponse::Ok().finish()
    } else {
        HttpResponse::InternalServerError().finish()
    }
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=debug");
    env_logger::init();
    
    run_migrations().await.unwrap();
    let db_pool = connect_db().await.unwrap();
    let app_state = web::Data::new(AppState { db_pool });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(LoggingMiddleware)
            .wrap(Logger::default())
            .route("/healthcheck", web::get().to(healthcheck))
            .route("/notes", web::get().to(get_notes))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

async fn run_migrations() -> std::result::Result<(), ErrorMigration> {
    println!("Running DB migrations...");
    let (mut client, con) = tokio_postgres::connect("host=localhost user=admin password=admin dbname=notes", NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = con.await {
            eprintln!("connection error: {}", e);
        }
    });
    let migration_report = embedded::migrations::runner()
        .run_async(&mut client)
        .await?;

    for migration in migration_report.applied_migrations() {
        println!(
            "Migration Applied -  Name: {}, Version: {}",
            migration.name(),
            migration.version()
        );
    }

    println!("DB migrations finished!");

    Ok(())
}