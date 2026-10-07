//! The pages of the site: Askama template structs, a `render_*` function for
//! each page, and the request handlers that `cargo run` uses.
//!
//! Each `#[derive(Template)]` struct is checked against its HTML file in
//! `templates/` when the code compiles, so a typo in a template is a compile
//! error rather than a broken page.
//!
//! Every page gets a `base` path that goes in front of its links: "" when the
//! site runs with `cargo run`, and "/CompSciRevision" (the repository name)
//! when it is exported for GitHub Pages, which serves it from that folder.

use crate::content::{Library, Topic};
use crate::exam::{AnswerBlock, AnswerKind, ExamTopic};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, web};
use askama::Template;

// ---------------------------------------------------------------------------
// Home
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage<'a> {
    base: &'a str,
    components: Vec<ComponentGroup<'a>>,
    /// How many exam questions there are in total.
    exam_questions: usize,
}

// The `'a` below is a lifetime: these structs borrow from the `Library`
// instead of copying it, and Rust checks the borrow is safe.
struct ComponentGroup<'a> {
    code: &'a str,
    title: &'a str,
    /// How many of this component's spec points have at least one topic.
    written: usize,
    total: usize,
    sections: Vec<SectionGroup<'a>>,
}

struct SectionGroup<'a> {
    number: &'a str,
    title: &'a str,
    points: Vec<PointRow<'a>>,
}

/// One spec point, with the topics written for it (often none yet).
struct PointRow<'a> {
    number: &'a str,
    title: &'a str,
    topics: Vec<&'a Topic>,
}

/// The home page: every spec point from spec.toml, grouped by component and section.
pub fn render_home(library: &Library, base: &str) -> askama::Result<String> {
    let components = library
        .spec
        .components
        .iter()
        .map(|component| {
            let sections: Vec<SectionGroup> = component
                .sections
                .iter()
                .map(|section| SectionGroup {
                    number: &section.number,
                    title: &section.title,
                    points: section
                        .points
                        .iter()
                        .map(|point| PointRow {
                            number: &point.number,
                            title: &point.title,
                            topics: library
                                .topics
                                .iter()
                                .filter(|t| t.point() == point.number)
                                .collect(),
                        })
                        .collect(),
                })
                .collect();
            let rows = sections.iter().flat_map(|s| &s.points);
            ComponentGroup {
                code: &component.code,
                title: &component.title,
                written: rows.clone().filter(|p| !p.topics.is_empty()).count(),
                total: rows.count(),
                sections,
            }
        })
        .collect();
    HomePage {
        base,
        components,
        exam_questions: library.exam.iter().map(|e| e.questions.len()).sum(),
    }
    .render()
}

// ---------------------------------------------------------------------------
// Topic notes, flashcards and quiz
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "topic.html")]
struct TopicPage<'a> {
    base: &'a str,
    topic: &'a Topic,
    /// The exam questions for this topic's spec point, if there are any.
    exam: Option<&'a ExamTopic>,
}

pub fn render_topic(library: &Library, topic: &Topic, base: &str) -> askama::Result<String> {
    TopicPage {
        base,
        topic,
        exam: library.exam_topic(topic.point()),
    }
    .render()
}

#[derive(Template)]
#[template(path = "flashcards.html")]
struct FlashcardsPage<'a> {
    base: &'a str,
    topic: &'a Topic,
}

pub fn render_flashcards(topic: &Topic, base: &str) -> askama::Result<String> {
    FlashcardsPage { base, topic }.render()
}

/// The quiz page carries every question, its options, answer and explanation.
/// `static/js/quiz.js` turns them into the one-question-at-a-time quiz in the
/// browser (shuffling, marking, score); without JavaScript they are listed
/// with a "Show answer" box each.
#[derive(Template)]
#[template(path = "quiz.html")]
struct QuizPage<'a> {
    base: &'a str,
    topic: &'a Topic,
}

pub fn render_quiz(topic: &Topic, base: &str) -> askama::Result<String> {
    QuizPage { base, topic }.render()
}

// ---------------------------------------------------------------------------
// Exam questions
// ---------------------------------------------------------------------------

/// Every spec point that has exam questions, grouped by component and section.
#[derive(Template)]
#[template(path = "exam_index.html")]
struct ExamIndexPage<'a> {
    base: &'a str,
    components: Vec<ExamComponent<'a>>,
    questions: usize,
}

struct ExamComponent<'a> {
    code: &'a str,
    title: &'a str,
    sections: Vec<ExamSection<'a>>,
}

struct ExamSection<'a> {
    number: &'a str,
    title: &'a str,
    topics: Vec<&'a ExamTopic>,
}

pub fn render_exam_index(library: &Library, base: &str) -> askama::Result<String> {
    let components = library
        .spec
        .components
        .iter()
        .map(|component| ExamComponent {
            code: &component.code,
            title: &component.title,
            sections: component
                .sections
                .iter()
                .map(|section| ExamSection {
                    number: &section.number,
                    title: &section.title,
                    topics: section
                        .points
                        .iter()
                        .filter_map(|point| library.exam_topic(&point.number))
                        .collect(),
                })
                .filter(|section| !section.topics.is_empty())
                .collect(),
        })
        .filter(|component: &ExamComponent| !component.sections.is_empty())
        .collect();
    ExamIndexPage {
        base,
        components,
        questions: library.exam.iter().map(|e| e.questions.len()).sum(),
    }
    .render()
}

/// All the questions for one spec point, each part with its mark scheme in a
/// "Mark scheme" box, and a box to write in your own mark (saved by exam.js).
#[derive(Template)]
#[template(path = "exam.html")]
struct ExamPage<'a> {
    base: &'a str,
    exam: &'a ExamTopic,
    /// Revision topics for the same spec point, to link to their notes.
    topics: Vec<&'a Topic>,
}

pub fn render_exam(library: &Library, exam: &ExamTopic, base: &str) -> askama::Result<String> {
    ExamPage {
        base,
        exam,
        topics: library
            .topics
            .iter()
            .filter(|t| t.point() == exam.spec)
            .collect(),
    }
    .render()
}

#[derive(Template)]
#[template(path = "404.html")]
struct NotFoundPage<'a> {
    base: &'a str,
}

pub fn render_not_found(base: &str) -> askama::Result<String> {
    NotFoundPage { base }.render()
}

// ---------------------------------------------------------------------------
// Request handlers for `cargo run` (base path "")
// ---------------------------------------------------------------------------

/// Turn a rendered page (or a template error) into an HTML response.
fn respond(page: askama::Result<String>, status: StatusCode) -> HttpResponse {
    match page {
        Ok(html) => HttpResponse::build(status)
            .content_type("text/html; charset=utf-8")
            .body(html),
        Err(error) => {
            eprintln!("Template error: {error}");
            HttpResponse::InternalServerError()
                .content_type("text/plain; charset=utf-8")
                .body("Sorry, something went wrong showing this page.")
        }
    }
}

pub async fn home(library: web::Data<Library>) -> HttpResponse {
    respond(render_home(&library, ""), StatusCode::OK)
}

pub async fn topic(library: web::Data<Library>, slug: web::Path<String>) -> HttpResponse {
    match library.topic(&slug) {
        Some(topic) => respond(render_topic(&library, topic, ""), StatusCode::OK),
        None => not_found_page(),
    }
}

pub async fn flashcards(library: web::Data<Library>, slug: web::Path<String>) -> HttpResponse {
    match library.topic(&slug) {
        Some(topic) => respond(render_flashcards(topic, ""), StatusCode::OK),
        None => not_found_page(),
    }
}

pub async fn quiz(library: web::Data<Library>, slug: web::Path<String>) -> HttpResponse {
    match library.topic(&slug) {
        Some(topic) => respond(render_quiz(topic, ""), StatusCode::OK),
        None => not_found_page(),
    }
}

pub async fn exam_index(library: web::Data<Library>) -> HttpResponse {
    respond(render_exam_index(&library, ""), StatusCode::OK)
}

pub async fn exam(library: web::Data<Library>, point: web::Path<String>) -> HttpResponse {
    match library.exam_topic(&point) {
        Some(exam) => respond(render_exam(&library, exam, ""), StatusCode::OK),
        None => not_found_page(),
    }
}

pub async fn not_found() -> HttpResponse {
    not_found_page()
}

fn not_found_page() -> HttpResponse {
    respond(render_not_found(""), StatusCode::NOT_FOUND)
}
