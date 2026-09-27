//! Loads everything in `content/` once at startup and checks it.
//!
//! The raw files are read into `Raw*` structs (exactly what is in the file),
//! checked, and then turned into the `Topic`, `Flashcard` and `Question`
//! structs the rest of the site uses, with Markdown already rendered to HTML.
//! Any problem becomes a `ContentError` that names the file and the problem.

use crate::spec::{self, Spec};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// What the rest of the site uses
// ---------------------------------------------------------------------------

/// All loaded content: the spec outline plus every topic, sorted by spec point.
pub struct Library {
    pub spec: Spec,
    pub topics: Vec<Topic>,
}

pub struct Topic {
    pub slug: String,
    pub title: String,
    pub spec: String,
    pub sample: bool,
    pub notes_html: String,
    pub flashcards: Vec<Flashcard>,
    pub quiz: Vec<Question>,
}

pub struct Flashcard {
    /// Stable id used as the localStorage key, made from the card's front text,
    /// so reordering cards in the file does not mix up a student's progress.
    pub id: String,
    pub front_html: String,
    pub back_html: String,
    pub spec: String,
}

pub struct Question {
    pub prompt_html: String,
    pub options_html: Vec<String>,
    /// 0-based index into `options_html`.
    pub answer: usize,
    pub explanation_html: String,
    pub spec: String,
    pub difficulty: u8,
    /// false keeps the options in file order (e.g. numbers in sequence).
    pub shuffle: bool,
}

impl Library {
    pub fn topic(&self, slug: &str) -> Option<&Topic> {
        self.topics.iter().find(|t| t.slug == slug)
    }
}

impl Topic {
    /// The spec point this topic covers, e.g. "1.4.3" for "1.4.3" or "1.4.3(b)".
    pub fn point(&self) -> &str {
        point_of(&self.spec)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A problem with one content file. Printed as `path: problem`.
#[derive(Debug)]
pub struct ContentError {
    pub file: PathBuf,
    pub problem: String,
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file.display(), self.problem)
    }
}

impl std::error::Error for ContentError {}

/// Shorthand for building an error about `file`.
pub(crate) fn problem(file: &Path, message: impl Into<String>) -> ContentError {
    ContentError {
        file: file.to_path_buf(),
        problem: message.into(),
    }
}

// ---------------------------------------------------------------------------
// What is in the files
// ---------------------------------------------------------------------------

// `deny_unknown_fields` turns a typo such as `anwser = 2` into an error
// instead of silently ignoring it.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontMatter {
    title: String,
    spec: String,
    #[serde(default)]
    sample: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FlashcardFile {
    #[serde(default)]
    card: Vec<RawCard>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCard {
    front: String,
    back: String,
    spec: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuizFile {
    #[serde(default)]
    question: Vec<RawQuestion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawQuestion {
    prompt: String,
    options: Vec<String>,
    answer: usize,
    explanation: String,
    spec: String,
    difficulty: u8,
    #[serde(default = "default_true")]
    shuffle: bool,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load and check everything under `dir` (normally `content/`).
pub fn load(dir: &Path) -> Result<Library, ContentError> {
    let spec = spec::load(&dir.join("spec.toml"))?;

    let entries =
        fs::read_dir(dir).map_err(|e| problem(dir, format!("cannot read folder: {e}")))?;
    let mut topic_dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| problem(dir, format!("cannot read folder: {e}")))?;
        if entry.path().is_dir() {
            topic_dirs.push(entry.path());
        }
    }

    let mut topics = Vec::new();
    for topic_dir in topic_dirs {
        topics.push(load_topic(&topic_dir, &spec)?);
    }

    // Sort by spec point, comparing the numbers so that 1.10 comes after 1.9.
    topics.sort_by_key(|topic| spec_sort_key(&topic.spec));

    Ok(Library { spec, topics })
}

fn load_topic(dir: &Path, spec: &Spec) -> Result<Topic, ContentError> {
    let slug = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    if !is_valid_slug(&slug) {
        return Err(problem(
            dir,
            "folder name must use only lowercase letters, digits and hyphens",
        ));
    }

    // Notes: YAML front matter, then Markdown.
    let notes_path = dir.join("notes.md");
    let notes_text = read_file(&notes_path)?;
    let (front_matter, body) = split_front_matter(&notes_text).ok_or_else(|| {
        problem(
            &notes_path,
            "must start with front matter between --- lines",
        )
    })?;
    let meta: FrontMatter = serde_saphyr::from_str(front_matter)
        .map_err(|e| problem(&notes_path, format!("front matter: {e}")))?;
    if meta.title.trim().is_empty() {
        return Err(problem(&notes_path, "title is empty"));
    }
    check_spec(&notes_path, "front matter", &meta.spec, spec)?;
    if body.trim().is_empty() {
        return Err(problem(
            &notes_path,
            "there are no notes after the front matter",
        ));
    }

    let flashcards = load_flashcards(&dir.join("flashcards.toml"), spec)?;
    let quiz = load_quiz(&dir.join("quiz.toml"), spec)?;

    Ok(Topic {
        slug,
        title: meta.title,
        spec: meta.spec,
        sample: meta.sample,
        notes_html: markdown_to_html(body),
        flashcards,
        quiz,
    })
}

fn load_flashcards(path: &Path, spec: &Spec) -> Result<Vec<Flashcard>, ContentError> {
    let file: FlashcardFile = parse_toml(path)?;
    if file.card.is_empty() {
        return Err(problem(path, "has no [[card]] entries"));
    }

    let mut cards: Vec<Flashcard> = Vec::new();
    for (i, raw) in file.card.into_iter().enumerate() {
        let label = format!("card {}", i + 1);
        check_not_empty(path, &label, "front", &raw.front)?;
        check_not_empty(path, &label, "back", &raw.back)?;
        check_spec(path, &label, &raw.spec, spec)?;

        let id = card_id(&raw.front);
        if cards.iter().any(|c| c.id == id) {
            return Err(problem(
                path,
                format!("{label}: same front as an earlier card"),
            ));
        }
        cards.push(Flashcard {
            id,
            front_html: markdown_to_html(&raw.front),
            back_html: markdown_to_html(&raw.back),
            spec: raw.spec,
        });
    }
    Ok(cards)
}

fn load_quiz(path: &Path, spec: &Spec) -> Result<Vec<Question>, ContentError> {
    let file: QuizFile = parse_toml(path)?;
    if file.question.is_empty() {
        return Err(problem(path, "has no [[question]] entries"));
    }

    let mut questions = Vec::new();
    for (i, raw) in file.question.into_iter().enumerate() {
        let label = format!("question {}", i + 1);
        check_not_empty(path, &label, "prompt", &raw.prompt)?;
        check_not_empty(path, &label, "explanation", &raw.explanation)?;
        check_spec(path, &label, &raw.spec, spec)?;

        if raw.options.len() < 2 {
            return Err(problem(path, format!("{label}: needs at least 2 options")));
        }
        for (j, option) in raw.options.iter().enumerate() {
            check_not_empty(path, &label, &format!("option {}", j + 1), option)?;
            if raw.options[..j].contains(option) {
                return Err(problem(
                    path,
                    format!("{label}: option \"{option}\" appears twice"),
                ));
            }
        }
        if raw.answer >= raw.options.len() {
            return Err(problem(
                path,
                format!(
                    "{label}: answer is {} but there are only {} options (answer counts from 0)",
                    raw.answer,
                    raw.options.len()
                ),
            ));
        }
        if !(1..=3).contains(&raw.difficulty) {
            return Err(problem(
                path,
                format!(
                    "{label}: difficulty is {} but must be 1, 2 or 3",
                    raw.difficulty
                ),
            ));
        }
        // Options are shuffled, so "option B" would point at the wrong answer.
        if let Some(phrase) = mentions_option_letter(&raw.explanation) {
            return Err(problem(
                path,
                format!(
                    "{label}: explanation says \"{phrase}\"; options are shuffled, so describe the answer itself"
                ),
            ));
        }

        questions.push(Question {
            prompt_html: markdown_to_html(&raw.prompt),
            options_html: raw.options.iter().map(|o| inline_markdown(o)).collect(),
            answer: raw.answer,
            explanation_html: markdown_to_html(&raw.explanation),
            spec: raw.spec,
            difficulty: raw.difficulty,
            shuffle: raw.shuffle,
        });
    }
    Ok(questions)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Read a text file, turning Windows line endings into plain `\n`.
fn read_file(path: &Path) -> Result<String, ContentError> {
    let text = fs::read_to_string(path).map_err(|e| problem(path, format!("cannot read: {e}")))?;
    Ok(text.replace("\r\n", "\n"))
}

/// Read and parse a TOML file into any struct that derives `Deserialize`.
/// (`T` is a generic type: the caller decides which struct to parse into.)
pub(crate) fn parse_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ContentError> {
    let text = read_file(path)?;
    toml::from_str(&text).map_err(|e| problem(path, e.to_string()))
}

/// Split `---\nfront matter\n---\nbody` into its two parts.
fn split_front_matter(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n").or_else(|| {
        // Allow a file that ends right after the closing ---.
        rest.strip_suffix("\n---").map(|r| r.len())
    })?;
    let front = &rest[..end];
    let body = rest.get(end + "\n---\n".len()..).unwrap_or("");
    Some((front, body))
}

fn check_not_empty(path: &Path, label: &str, field: &str, value: &str) -> Result<(), ContentError> {
    if value.trim().is_empty() {
        Err(problem(path, format!("{label}: {field} is empty")))
    } else {
        Ok(())
    }
}

/// Check a `spec` value looks right and names a point in `spec.toml`.
fn check_spec(path: &Path, label: &str, reference: &str, spec: &Spec) -> Result<(), ContentError> {
    if !is_valid_spec(reference) {
        return Err(problem(
            path,
            format!("{label}: spec \"{reference}\" should look like \"1.4.3\" or \"1.4.3(a)\""),
        ));
    }
    spec.check_reference(reference)
        .map_err(|message| problem(path, format!("{label}: {message}")))
}

/// True for spec points like "1.4.3" or "1.4.3(a)".
pub fn is_valid_spec(spec: &str) -> bool {
    let numbers = match spec.split_once('(') {
        Some((numbers, letter)) => {
            let mut chars = letter.chars();
            let ok = matches!(
                (chars.next(), chars.next(), chars.next()),
                (Some(c), Some(')'), None) if c.is_ascii_lowercase()
            );
            if !ok {
                return false;
            }
            numbers
        }
        None => spec,
    };
    let parts: Vec<&str> = numbers.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| is_number(p))
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// "1.4.3(a)" -> "1.4.3"
pub fn point_of(spec: &str) -> &str {
    match spec.split_once('(') {
        Some((point, _)) => point,
        None => spec,
    }
}

/// "1.10.2" -> [1, 10, 2], so spec points sort in number order.
fn spec_sort_key(spec: &str) -> Vec<u32> {
    spec.split(['.', '('])
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Finds phrases like "option B" or "answer (c)" that refer to an option by letter.
fn mentions_option_letter(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for pair in words.windows(2) {
        let first = pair[0].to_lowercase();
        let letter = pair[1].trim_matches(|c: char| !c.is_ascii_alphanumeric());
        let is_option_word = first == "option" || first == "answer";
        let is_letter = matches!(
            letter,
            "A" | "B" | "C" | "D" | "E" | "a" | "b" | "c" | "d" | "e"
        ) && (letter.chars().all(|c| c.is_ascii_uppercase())
            || pair[1].starts_with('('));
        if is_option_word && is_letter {
            return Some(format!("{} {}", pair[0], pair[1]));
        }
    }
    None
}

/// A short, stable id for a flashcard, made from its front text.
///
/// This is the FNV-1a hash. We write it out rather than using Rust's built-in
/// hasher because the built-in one may change between Rust versions, which
/// would reset every student's saved progress.
fn card_id(front: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in front.trim().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

/// The only HTML tags allowed inside Markdown content.
const ALLOWED_TAGS: [&str; 4] = ["<sup>", "</sup>", "<sub>", "</sub>"];

/// Render Markdown to HTML.
///
/// Raw HTML in the Markdown is shown as text rather than run, so a stray `<`
/// in content can never break the page. The one exception is `<sup>` and
/// `<sub>` (for 2<sup>8</sup> or 1011<sub>2</sub>). Tables are wrapped in a
/// scrolling box so a wide truth table does not push the page sideways on a phone.
pub fn markdown_to_html(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let events = Parser::new_ext(markdown, options).flat_map(|event| match event {
        Event::InlineHtml(tag) if ALLOWED_TAGS.contains(&tag.as_ref()) => {
            vec![Event::InlineHtml(tag)]
        }
        Event::Html(text) | Event::InlineHtml(text) => vec![Event::Text(text)],
        Event::Start(Tag::Table(alignment)) => vec![
            Event::Html(r#"<div class="table-wrap" tabindex="0">"#.into()),
            Event::Start(Tag::Table(alignment)),
        ],
        Event::End(TagEnd::Table) => vec![Event::End(TagEnd::Table), Event::Html("</div>".into())],
        other => vec![other],
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// Render a short piece of Markdown (a quiz option) without the wrapping <p>.
pub fn inline_markdown(markdown: &str) -> String {
    let rendered = markdown_to_html(markdown);
    let trimmed = rendered.trim_end();
    match trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
    {
        Some(inner) if !inner.contains("<p>") => inner.to_string(),
        _ => rendered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_formats() {
        assert!(is_valid_spec("1.4.3"));
        assert!(is_valid_spec("2.1.10"));
        assert!(is_valid_spec("1.4.3(a)"));
        assert!(!is_valid_spec("1.4"));
        assert!(!is_valid_spec("1.4.x"));
        assert!(!is_valid_spec("1.4.3(A)"));
        assert!(!is_valid_spec(""));
        assert_eq!(point_of("1.4.3(a)"), "1.4.3");
        assert_eq!(point_of("1.4.3"), "1.4.3");
    }

    #[test]
    fn spec_points_sort_as_numbers() {
        assert!(spec_sort_key("1.9.1") < spec_sort_key("1.10.1"));
    }

    #[test]
    fn option_letters_are_spotted() {
        assert!(mentions_option_letter("So option B is right.").is_some());
        assert!(mentions_option_letter("The answer (c) is right.").is_some());
        assert!(mentions_option_letter("The answer a student gives").is_none());
        assert!(mentions_option_letter("This option always works.").is_none());
    }

    #[test]
    fn front_matter_is_split() {
        let (front, body) = split_front_matter("---\ntitle: X\n---\nHello").unwrap();
        assert_eq!(front, "title: X");
        assert_eq!(body, "Hello");
        assert!(split_front_matter("title: X\nHello").is_none());
    }

    #[test]
    fn raw_html_is_escaped() {
        let html = markdown_to_html("a <script>alert(1)</script> b");
        assert!(!html.contains("<script>"));
        let html = markdown_to_html("a <b onclick=\"x\">b</b>");
        assert!(!html.contains("<b"));
        // Superscript and subscript are allowed for powers and number bases.
        assert!(markdown_to_html("2<sup>8</sup>").contains("2<sup>8</sup>"));
        assert!(markdown_to_html("1011<sub>2</sub>").contains("<sub>2</sub>"));
    }

    #[test]
    fn options_render_without_paragraph() {
        assert_eq!(inline_markdown("`1011`"), "<code>1011</code>");
    }

    #[test]
    fn card_ids_are_stable() {
        // If this ever changes, every student's saved flashcard progress resets.
        assert_eq!(card_id("What is the ALU?"), card_id(" What is the ALU? "));
        assert_eq!(card_id(""), "cbf29ce484222325");
    }
}
