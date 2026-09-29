//! The revision site as a library, so `main.rs` and the tests build the same app.

pub mod content;
pub mod exam;
pub mod export;
pub mod routes;
pub mod spec;

use actix_web::web;
use content::{ContentError, Library};
use std::path::Path;

/// Folder the content is loaded from, relative to where `cargo run` is started.
pub const CONTENT_DIR: &str = "content";
/// Folder the OCR exam questions are loaded from (made by tools/extract-exam-questions).
pub const EXAM_DIR: &str = "exam-questions";
/// Folder served at `/static`. Only this folder is served, never `sources/`.
pub const STATIC_DIR: &str = "static";

/// Load everything the site shows: `content/` and the exam questions.
pub fn load_site() -> Result<Library, ContentError> {
    let mut library = content::load(Path::new(CONTENT_DIR))?;
    library.exam = exam::load(Path::new(EXAM_DIR), &library.spec)?;
    Ok(library)
}

/// Register every route. `main.rs` and the tests both call this.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(actix_files::Files::new("/static", STATIC_DIR))
        .route("/", web::get().to(routes::home))
        .route("/topics/{slug}", web::get().to(routes::topic))
        .route(
            "/topics/{slug}/flashcards",
            web::get().to(routes::flashcards),
        )
        .route("/topics/{slug}/quiz", web::get().to(routes::quiz))
        .route("/exam", web::get().to(routes::exam_index))
        .route("/exam/{point}", web::get().to(routes::exam))
        .default_service(web::to(routes::not_found));
}
