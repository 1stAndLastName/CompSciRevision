//! The revision site as a library, so `main.rs` and the tests build the same app.

pub mod content;
pub mod routes;
pub mod spec;

use actix_web::web;

/// Folder the content is loaded from, relative to where `cargo run` is started.
pub const CONTENT_DIR: &str = "content";
/// Folder served at `/static`. Only this folder is served, never `sources/`.
pub const STATIC_DIR: &str = "static";

/// Register every route. `main.rs` and the tests both call this.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(actix_files::Files::new("/static", STATIC_DIR))
        .route("/", web::get().to(routes::home))
        .route("/topics/{slug}", web::get().to(routes::topic))
        .route(
            "/topics/{slug}/flashcards",
            web::get().to(routes::flashcards),
        )
        .route("/topics/{slug}/quiz", web::get().to(routes::quiz_start))
        .route(
            "/topics/{slug}/quiz/answer",
            web::post().to(routes::quiz_answer),
        )
        .route("/topics/{slug}/quiz/next", web::get().to(routes::quiz_next))
        .default_service(web::to(routes::not_found));
}
