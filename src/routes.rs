//! Request handlers and the Askama template structs they fill in.
//!
//! Each `#[derive(Template)]` struct is checked against its HTML file in
//! `templates/` when the code compiles, so a typo in a template is a compile
//! error rather than a broken page.

use crate::content::{Library, Question, Topic};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, web};
use askama::Template;
use rand::seq::SliceRandom;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

/// Render a template into an HTML response with the given status code.
fn render(template: &impl Template, status: StatusCode) -> HttpResponse {
    match template.render() {
        Ok(html) => HttpResponse::build(status)
            .content_type("text/html; charset=utf-8")
            // The quiz URLs answer with a fragment or a full page depending on
            // this header, so caches must keep the two apart.
            .insert_header(("Vary", "HX-Request"))
            .body(html),
        Err(error) => {
            eprintln!("Template error: {error}");
            HttpResponse::InternalServerError()
                .content_type("text/plain; charset=utf-8")
                .body("Sorry, something went wrong showing this page.")
        }
    }
}

fn ok(template: &impl Template) -> HttpResponse {
    render(template, StatusCode::OK)
}

/// True when the request came from htmx, which wants just a page fragment.
fn is_htmx(req: &HttpRequest) -> bool {
    req.headers().contains_key("HX-Request")
}

// ---------------------------------------------------------------------------
// Home
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage<'a> {
    components: Vec<ComponentGroup<'a>>,
}

// The `'a` below is a lifetime: these structs borrow topics from the
// `Library` instead of copying them, and Rust checks the borrow is safe.
struct ComponentGroup<'a> {
    code: &'a str,
    title: &'a str,
    sections: Vec<SectionGroup<'a>>,
}

struct SectionGroup<'a> {
    spec: &'a str,
    title: &'a str,
    topics: Vec<&'a Topic>,
}

pub async fn home(library: web::Data<Library>) -> HttpResponse {
    let components = library
        .components
        .iter()
        .map(|component| ComponentGroup {
            code: &component.code,
            title: &component.title,
            sections: component
                .sections
                .iter()
                .map(|section| SectionGroup {
                    spec: &section.spec,
                    title: &section.title,
                    topics: library
                        .topics
                        .iter()
                        .filter(|t| t.section() == section.spec)
                        .collect(),
                })
                // Only show sections that have at least one topic.
                .filter(|s| !s.topics.is_empty())
                .collect(),
        })
        .collect();
    ok(&HomePage { components })
}

// ---------------------------------------------------------------------------
// Topic notes and flashcards
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "topic.html")]
struct TopicPage<'a> {
    topic: &'a Topic,
}

pub async fn topic(library: web::Data<Library>, slug: web::Path<String>) -> HttpResponse {
    match library.topic(&slug) {
        Some(topic) => ok(&TopicPage { topic }),
        None => not_found_page(),
    }
}

#[derive(Template)]
#[template(path = "flashcards.html")]
struct FlashcardsPage<'a> {
    topic: &'a Topic,
}

pub async fn flashcards(library: web::Data<Library>, slug: web::Path<String>) -> HttpResponse {
    match library.topic(&slug) {
        Some(topic) => ok(&FlashcardsPage { topic }),
        None => not_found_page(),
    }
}

// ---------------------------------------------------------------------------
// Quiz
//
// The server keeps no record of a student's quiz. Each form carries the
// question order, the position and the score so far in hidden fields, and
// the server works out the next step from those.
// ---------------------------------------------------------------------------

/// The full quiz page. `step_html` is one of the three steps below.
#[derive(Template)]
#[template(path = "quiz.html")]
struct QuizPage<'a> {
    topic: &'a Topic,
    step_html: String,
}

/// Step 1: a question with its options.
#[derive(Template)]
#[template(path = "quiz_ask.html")]
struct QuizAsk<'a> {
    topic: &'a Topic,
    question: &'a Question,
    number: usize,
    total: usize,
    score: usize,
    order: String,
    /// The options in the order shown: (index in the file, HTML).
    options: Vec<(usize, &'a str)>,
    option_order: String,
}

/// Step 2: whether the answer was right, and why.
#[derive(Template)]
#[template(path = "quiz_feedback.html")]
struct QuizFeedback<'a> {
    topic: &'a Topic,
    question: &'a Question,
    number: usize,
    total: usize,
    score: usize,
    order: String,
    correct: bool,
    options: Vec<FeedbackOption<'a>>,
}

struct FeedbackOption<'a> {
    html: &'a str,
    chosen: bool,
    correct: bool,
}

/// Step 3: the final score.
#[derive(Template)]
#[template(path = "quiz_summary.html")]
struct QuizSummary<'a> {
    topic: &'a Topic,
    score: usize,
    total: usize,
}

impl QuizSummary<'_> {
    fn message(&self) -> &'static str {
        if self.score == self.total {
            "Full marks. Well done!"
        } else if self.score * 3 >= self.total * 2 {
            "Good work. Check the ones you missed and try again."
        } else {
            "Worth another look. Read the notes, then have another go."
        }
    }
}

#[derive(Deserialize)]
pub struct AnswerForm {
    order: String,
    opts: String,
    pos: usize,
    score: usize,
    choice: usize,
}

#[derive(Deserialize)]
pub struct NextQuery {
    order: String,
    pos: usize,
    score: usize,
}

/// Start the quiz with the questions in a new random order.
pub async fn quiz_start(library: web::Data<Library>, slug: web::Path<String>) -> HttpResponse {
    let Some(topic) = library.topic(&slug) else {
        return not_found_page();
    };
    let mut order: Vec<usize> = (0..topic.quiz.len()).collect();
    order.shuffle(&mut rand::rng());
    let step_html = ask_step(topic, &order, 0, 0).render().unwrap_or_default();
    ok(&QuizPage { topic, step_html })
}

/// Mark one answer and show the feedback.
pub async fn quiz_answer(
    req: HttpRequest,
    library: web::Data<Library>,
    slug: web::Path<String>,
    form: web::Form<AnswerForm>,
) -> HttpResponse {
    let Some(topic) = library.topic(&slug) else {
        return not_found_page();
    };
    let total = topic.quiz.len();
    let Some(order) = parse_order(&form.order, total) else {
        return bad_request();
    };
    if form.pos >= total || form.score > form.pos {
        return bad_request();
    }
    let question = &topic.quiz[order[form.pos]];
    let Some(option_order) = parse_order(&form.opts, question.options_html.len()) else {
        return bad_request();
    };
    if form.choice >= question.options_html.len() {
        return bad_request();
    }

    let correct = form.choice == question.answer;
    let feedback = QuizFeedback {
        topic,
        question,
        number: form.pos + 1,
        total,
        score: form.score + usize::from(correct),
        order: form.order.clone(),
        correct,
        options: option_order
            .iter()
            .map(|&i| FeedbackOption {
                html: &question.options_html[i],
                chosen: i == form.choice,
                correct: i == question.answer,
            })
            .collect(),
    };
    quiz_step(&req, topic, &feedback)
}

/// Show the next question, or the summary after the last one.
pub async fn quiz_next(
    req: HttpRequest,
    library: web::Data<Library>,
    slug: web::Path<String>,
    query: web::Query<NextQuery>,
) -> HttpResponse {
    let Some(topic) = library.topic(&slug) else {
        return not_found_page();
    };
    let total = topic.quiz.len();
    let Some(order) = parse_order(&query.order, total) else {
        return bad_request();
    };
    if query.pos > total || query.score > query.pos {
        return bad_request();
    }

    if query.pos == total {
        let summary = QuizSummary {
            topic,
            score: query.score,
            total,
        };
        quiz_step(&req, topic, &summary)
    } else {
        quiz_step(
            &req,
            topic,
            &ask_step(topic, &order, query.pos, query.score),
        )
    }
}

/// Build the question at `pos`, shuffling its options unless the file says not to.
fn ask_step<'a>(topic: &'a Topic, order: &[usize], pos: usize, score: usize) -> QuizAsk<'a> {
    let question = &topic.quiz[order[pos]];
    let mut option_order: Vec<usize> = (0..question.options_html.len()).collect();
    if question.shuffle {
        option_order.shuffle(&mut rand::rng());
    }
    QuizAsk {
        topic,
        question,
        number: pos + 1,
        total: order.len(),
        score,
        order: join_numbers(order),
        options: option_order
            .iter()
            .map(|&i| (i, question.options_html[i].as_str()))
            .collect(),
        option_order: join_numbers(&option_order),
    }
}

/// htmx gets just the step; a browser without JavaScript gets the whole page.
fn quiz_step(req: &HttpRequest, topic: &Topic, step: &impl Template) -> HttpResponse {
    if is_htmx(req) {
        return ok(step);
    }
    match step.render() {
        Ok(step_html) => ok(&QuizPage { topic, step_html }),
        Err(error) => {
            eprintln!("Template error: {error}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

/// Parse "2,0,1" and check it contains each number from 0 to len-1 exactly once.
fn parse_order(text: &str, len: usize) -> Option<Vec<usize>> {
    let order: Vec<usize> = text
        .split(',')
        .map(|n| n.trim().parse().ok())
        .collect::<Option<_>>()?;
    let mut sorted = order.clone();
    sorted.sort_unstable();
    if sorted == (0..len).collect::<Vec<_>>() {
        Some(order)
    } else {
        None
    }
}

fn join_numbers(numbers: &[usize]) -> String {
    numbers
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn bad_request() -> HttpResponse {
    HttpResponse::BadRequest()
        .content_type("text/plain; charset=utf-8")
        .body("That quiz link is out of date. Go back to the topic and start the quiz again.")
}

// ---------------------------------------------------------------------------
// 404
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "404.html")]
struct NotFoundPage;

pub async fn not_found() -> HttpResponse {
    not_found_page()
}

fn not_found_page() -> HttpResponse {
    render(&NotFoundPage, StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_must_be_a_permutation() {
        assert_eq!(parse_order("2,0,1", 3), Some(vec![2, 0, 1]));
        assert_eq!(parse_order("0,0,1", 3), None);
        assert_eq!(parse_order("0,1", 3), None);
        assert_eq!(parse_order("0,1,x", 3), None);
        assert_eq!(parse_order("0,1,3", 3), None);
    }
}
