//! Every route returns the right status code, using the real content.

use actix_web::http::StatusCode;
use actix_web::{App, body::to_bytes, test, web};
use revision_site::{configure, content};
use std::path::Path;

/// Build the app exactly as `main.rs` does and send it one request.
async fn send(req: test::TestRequest) -> (StatusCode, String) {
    let library = content::load(Path::new("content")).unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(library))
            .configure(configure),
    )
    .await;
    let response = test::call_service(&app, req.to_request()).await;
    let status = response.status();
    let body = to_bytes(response.into_body()).await.unwrap_or_default();
    (status, String::from_utf8_lossy(&body).into_owned())
}

async fn get(uri: &str) -> (StatusCode, String) {
    send(test::TestRequest::get().uri(uri)).await
}

/// The slug of the first topic.
fn first_slug() -> String {
    content::load(Path::new("content")).unwrap().topics[0]
        .slug
        .clone()
}

#[actix_web::test]
async fn pages_return_200() {
    let slug = first_slug();
    for uri in [
        "/".to_string(),
        format!("/topics/{slug}"),
        format!("/topics/{slug}/flashcards"),
        format!("/topics/{slug}/quiz"),
        "/static/css/site.css".to_string(),
        "/static/js/quiz.js".to_string(),
        "/static/manifest.webmanifest".to_string(),
        "/static/icons/icon-192.png".to_string(),
        "/static/icons/icon-512.png".to_string(),
        "/static/icons/icon-maskable-512.png".to_string(),
        "/static/icons/apple-touch-icon.png".to_string(),
    ] {
        let (status, _) = get(&uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
}

#[actix_web::test]
async fn home_lists_every_spec_point() {
    let (status, body) = get("/").await;
    assert_eq!(status, StatusCode::OK);
    let library = content::load(Path::new("content")).unwrap();
    for point in library.spec.points() {
        assert!(
            body.contains(&point.number),
            "home should list {}",
            point.number
        );
    }
    // 2.3.1 has no topic yet, so it is listed without a link.
    assert!(body.contains("Not written yet"));
}

#[actix_web::test]
async fn unknown_pages_return_404() {
    for uri in [
        "/no-such-page",
        "/topics/no-such-topic",
        "/topics/no-such-topic/flashcards",
        "/topics/no-such-topic/quiz",
        "/static/no-such-file.css",
        "/sources/textbook.pdf",
        "/content/spec.toml",
    ] {
        let (status, body) = get(uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        if !uri.starts_with("/static") {
            assert!(
                body.contains("Page not found"),
                "{uri} should show the 404 page"
            );
        }
    }
}

#[actix_web::test]
async fn quiz_page_carries_every_question_and_answer() {
    let library = content::load(Path::new("content")).unwrap();
    for topic in &library.topics {
        let (status, body) = get(&format!("/topics/{}/quiz", topic.slug)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body.matches("class=\"quiz-q\"").count(),
            topic.quiz.len(),
            "{}: every question should be on the page",
            topic.slug
        );
        assert_eq!(body.matches("Show answer").count(), topic.quiz.len());
        assert!(body.contains("static/js/quiz.js"));
    }
}
