//! Every route returns the right status code, using the real content.

use actix_web::http::StatusCode;
use actix_web::{App, body::to_bytes, test, web};
use revision_site::exam::AnswerBlock;
use revision_site::{configure, load_site};

/// Build the app exactly as `main.rs` does and send it one request.
async fn send(req: test::TestRequest) -> (StatusCode, String) {
    let library = load_site().unwrap();
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
    load_site().unwrap().topics[0].slug.clone()
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
        "/static/js/exam.js".to_string(),
        "/exam".to_string(),
        "/exam/1.4.3".to_string(),
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
    let library = load_site().unwrap();
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
        "/exam/9.9.9",
        "/exam-questions/1.4.3.toml",
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
    let library = load_site().unwrap();
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

#[actix_web::test]
async fn exam_page_carries_every_question_and_mark_scheme() {
    let library = load_site().unwrap();
    assert!(!library.exam.is_empty(), "exam-questions/ should load");
    for exam in &library.exam {
        let (status, body) = get(&format!("/exam/{}", exam.spec)).await;
        assert_eq!(status, StatusCode::OK, "{}", exam.spec);
        assert_eq!(
            body.matches("class=\"exam-q\"").count(),
            exam.questions.len(),
            "{}: every question should be on the page",
            exam.spec
        );
        let boxes: usize = exam
            .questions
            .iter()
            .map(|q| {
                q.parts.iter().filter(|p| !p.mark_scheme.is_empty()).count()
                    + usize::from(!q.other_mark_scheme.is_empty())
            })
            .sum();
        assert_eq!(
            body.matches("class=\"mark-scheme\"").count(),
            boxes,
            "{}",
            exam.spec
        );
        assert!(body.contains("static/js/exam.js"));
        let auto: usize = exam
            .questions
            .iter()
            .flat_map(|q| &q.parts)
            .filter(|p| p.auto.is_some())
            .count();
        assert_eq!(
            body.matches("data-auto>").count(),
            auto,
            "{}: every automatically marked part has answer boxes",
            exam.spec
        );
        // Bullet points in the mark scheme of a part marked by hand get tick boxes.
        let has_points = exam.questions.iter().flat_map(|q| &q.parts).any(|p| {
            p.auto.is_none()
                && p.marks.is_some()
                && p.mark_scheme.iter().any(|row| {
                    row.answer
                        .iter()
                        .any(|block| matches!(block, AnswerBlock::Points(_)))
                })
        });
        assert_eq!(body.contains("data-point="), has_points, "{}", exam.spec);
        assert!(
            body.contains("static/js/diagrams.js"),
            "exam pages draw their Mermaid diagrams"
        );
        assert!(body.contains("© OCR"), "the source should be credited");
    }
}

#[actix_web::test]
async fn exam_list_links_every_spec_point_with_questions() {
    let library = load_site().unwrap();
    let (status, body) = get("/exam").await;
    assert_eq!(status, StatusCode::OK);
    for exam in &library.exam {
        assert!(body.contains(&format!("href=\"/exam/{}\"", exam.spec)));
    }
    let (_, home) = get("/").await;
    assert!(
        home.contains("href=\"/exam\""),
        "home should link to the exam questions"
    );
}
