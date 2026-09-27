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

/// The first topic, and the numbers needed to answer its first question.
fn first_topic() -> (String, usize, usize, usize) {
    let library = content::load(Path::new("content")).unwrap();
    let topic = &library.topics[0];
    // Question order "0,1,2,..." so position 0 is question 0 in the file.
    let question = &topic.quiz[0];
    (
        topic.slug.clone(),
        topic.quiz.len(),
        question.options_html.len(),
        question.answer,
    )
}

fn numbers(n: usize) -> String {
    (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

#[actix_web::test]
async fn pages_return_200() {
    let (slug, ..) = first_topic();
    for uri in [
        "/".to_string(),
        format!("/topics/{slug}"),
        format!("/topics/{slug}/flashcards"),
        format!("/topics/{slug}/quiz"),
        "/static/css/site.css".to_string(),
        "/static/vendor/htmx-2.0.11.min.js".to_string(),
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
async fn right_and_wrong_answers_get_feedback() {
    let (slug, total, option_count, answer) = first_topic();
    let wrong = (answer + 1) % option_count;
    for (choice, expected) in [(answer, "Correct"), (wrong, "Not quite")] {
        let form = [
            ("order", numbers(total)),
            ("opts", numbers(option_count)),
            ("pos", "0".to_string()),
            ("score", "0".to_string()),
            ("choice", choice.to_string()),
        ];
        let req = test::TestRequest::post()
            .uri(&format!("/topics/{slug}/quiz/answer"))
            .set_form(form);
        let (status, body) = send(req).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body.contains(expected),
            "choice {choice} should say {expected}"
        );
    }
}

#[actix_web::test]
async fn htmx_requests_get_a_fragment() {
    let (slug, total, ..) = first_topic();
    let uri = format!(
        "/topics/{slug}/quiz/next?order={}&pos=1&score=1",
        numbers(total)
    );
    let (status, full_page) = get(&uri).await;
    assert_eq!(status, StatusCode::OK);
    assert!(full_page.contains("<html"));

    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("HX-Request", "true"));
    let (status, fragment) = send(req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!fragment.contains("<html"));
    assert!(fragment.contains("Question 2 of"));
}

#[actix_web::test]
async fn last_step_shows_the_score() {
    let (slug, total, ..) = first_topic();
    let uri = format!(
        "/topics/{slug}/quiz/next?order={}&pos={total}&score=2",
        numbers(total)
    );
    let (status, body) = get(&uri).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Your score"));
}

#[actix_web::test]
async fn tampered_quiz_links_return_400() {
    let (slug, total, ..) = first_topic();
    for query in [
        "order=0,0,0&pos=0&score=0".to_string(),
        format!("order={}&pos={}&score=0", numbers(total), total + 1),
        format!("order={}&pos=1&score=5", numbers(total)),
        "pos=0&score=0".to_string(),
    ] {
        let (status, _) = get(&format!("/topics/{slug}/quiz/next?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
    }
}
