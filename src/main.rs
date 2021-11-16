use std::{convert::Infallible, net::SocketAddr};

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use axum::{
    body::{Bytes, Full},
    error_handling::HandleErrorLayer,
    extract::{Extension, Path, Query},
    handler::Handler,
    http::{Response, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;

use tower::{BoxError, ServiceBuilder};
use tower_http::{add_extension::AddExtensionLayer, trace::TraceLayer};

use askama::Template;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[tokio::main]
async fn main() {

    // Set the RUST_LOG, if it hasn't been explicitly defined
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "little_nova=debug,tower_http=debug")
    }
    let tmp = std::env::var_os("RUST_LOG");
    println!("RUST_LOG = {:?}", &tmp);

    // Setup tracing
    tracing_subscriber::fmt::init();

    let db = Db::default();

    let app = Router::new()
        .route("/", get(get_comment_entries))
        .route("/create", post(create_comment))
        .route("/:id", get(get_comment))
        // Add a handler_404 for routes to unknown paths
        .fallback(handler_404.into_service())
        // Add middleware to all routes
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|error: BoxError| {
                    if error.is::<tower::timeout::error::Elapsed>() {
                        Ok(StatusCode::REQUEST_TIMEOUT)
                    } else {
                        Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Unhandled internal error: {}", error),
                        ))
                    }
                }))
                .timeout(Duration::from_secs(10))
                .layer(TraceLayer::new_for_http())
                .layer(AddExtensionLayer::new(db))
                .into_inner(),
        );

    // run it
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    tracing::debug!("listening on {}", addr);

    // Rustls
    // Need private key and crt file
    let config = RustlsConfig::from_pem_file("./certs/server.crt", "./certs/server.key")
        .await
        .unwrap();

    let handle = Handle::new();

    // Spawn a task to shutdown server.
    tokio::spawn(graceful_shutdown(handle.clone()));

    // HTTPS (HTTP/2) communication
    axum_server::bind_rustls(addr, config)
        .handle(handle)
        .serve(app.into_make_service())
        .await
        .unwrap();

}

//Structure for create comment
#[derive(Debug, Deserialize)]
struct CreateTodo {
    text: String,
}

// The query parameters for comment index
#[derive(Debug, Deserialize, Default)]
pub struct Pagination {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

async fn get_comment(
    Path(id): Path<Uuid>,
    Extension(db): Extension<Db>,
) -> Result<impl IntoResponse, StatusCode> {
    let comment = db
        .read()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;

    let id = comment.id;
    let name = comment.name;
    let text = comment.text;
    let utc = comment.utc;

    let template = CommentTemplate {
        id,
        name,
        text,
        utc,
    };

    Ok(HtmlTemplate(template).into_response())
}

async fn get_comment_entries(
    pagination: Option<Query<Pagination>>, // Query string
    Extension(db): Extension<Db>,
) -> impl IntoResponse {
    let comment = db.read().unwrap();

    let Query(pagination) = pagination.unwrap_or_default();

    let total = comment.len();

    let mut comment_entries = comment
        .values()
        .cloned()
        .skip(pagination.offset.unwrap_or(0))
        .take(pagination.limit.unwrap_or(100_usize))
        .collect::<Vec<_>>();
    // Sort by newest transmission date  (descending order)
    comment_entries.sort_by(|a, b| b.utc.cmp(&a.utc));
    let entries = comment_entries;
    let template = CommentEntriesTemplate { total, entries };

    HtmlTemplate(template).into_response()
}

#[derive(Debug, Deserialize)]
struct CreateComment {
    name: String,
    text: String,
    utc: DateTime<Utc>,
}

async fn create_comment(
    Json(input): Json<CreateComment>,
    Extension(db): Extension<Db>,
) -> impl IntoResponse {
    let comment = Comment {
        id: Uuid::new_v4(),
        name: input.name,
        text: input.text,
        utc: input.utc,
    };

    db.write().unwrap().insert(comment.id, comment.clone());

    (StatusCode::CREATED, Json(comment))
}

#[derive(Debug, Serialize, Clone)]
struct Comment {
    id: Uuid,
    name: String,
    text: String,
    // Receive in ISO format
    utc: DateTime<Utc>,
}

type Db = Arc<RwLock<HashMap<Uuid, Comment>>>;

#[derive(Template)]
#[template(path = "comment-entries.html")]
struct CommentEntriesTemplate {
    // Total number of comments
    total: usize,
    // Comment entries
    entries: Vec<Comment>,
}

#[derive(Template)]
#[template(path = "comment.html")]
struct CommentTemplate {
    id: Uuid,
    name: String,
    text: String,
    utc: DateTime<Utc>,
}

struct HtmlTemplate<T>(T);

impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    type Body = Full<Bytes>;
    type BodyError = Infallible;

    fn into_response(self) -> Response<Self::Body> {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::from(format!(
                    "Failed to render template. Error: {}",
                    err
                )))
                .unwrap(),
        }
    }
}

#[cfg(unix)]
async fn graceful_shutdown(handle: Handle) {
    use std::io;
    use tokio::signal::unix::SignalKind;

    async fn terminate() -> io::Result<()> {
        tokio::signal::unix::signal(SignalKind::terminate())?
            .recv()
            .await;
        Ok(())
    }

    // The tokio::select! is return when either fn terminate or tokio::signal::ctrl_c() is completed
    // Press ctrl_c or kill command to receive the signal
    tokio::select! {
        _ = terminate() => {},
        _ = tokio::signal::ctrl_c() => {},
    };
    println!("signal received, starting graceful shutdown");

    // Signal the server to shutdown using Handle.
    handle.graceful_shutdown(Some(Duration::from_secs(30)));
}

#[cfg(windows)]
async fn graceful_shutdown(handle: Handle) {
    tokio::signal::ctrl_c()
        .await
        .expect("faild to install CTRL+C handler");
    println!("signal received, starting graceful shutdown");
    // Signal the server to shutdown using Handle.
    handle.graceful_shutdown(Some(Duration::from_secs(30)));
}

// The global 404 handler
async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "404 not found")
}
