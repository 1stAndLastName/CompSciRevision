//! Tests for the content loader: the real `content/` folder, plus broken
//! copies of a small topic to check each kind of mistake is caught and named.

use revision_site::content::{self, is_valid_spec};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn real_content_loads() {
    let library = content::load(Path::new("content")).expect("content/ should load");
    assert!(
        !library.topics.is_empty(),
        "there should be at least one topic"
    );
}

#[test]
fn every_item_has_a_valid_spec_reference() {
    let library = content::load(Path::new("content")).unwrap();
    for topic in &library.topics {
        assert!(is_valid_spec(&topic.spec), "{}: bad spec", topic.slug);
        for card in &topic.flashcards {
            assert!(is_valid_spec(&card.spec), "{}: card spec", topic.slug);
        }
        for question in &topic.quiz {
            assert!(
                is_valid_spec(&question.spec),
                "{}: question spec",
                topic.slug
            );
        }
    }
}

#[test]
fn every_quiz_answer_index_is_valid() {
    let library = content::load(Path::new("content")).unwrap();
    for topic in &library.topics {
        for (i, question) in topic.quiz.iter().enumerate() {
            assert!(
                question.answer < question.options_html.len(),
                "{} question {}: answer out of range",
                topic.slug,
                i + 1
            );
            assert!((1..=3).contains(&question.difficulty));
        }
    }
}

// ---------------------------------------------------------------------------
// Broken content
// ---------------------------------------------------------------------------

const SPEC: &str = r#"
[[component]]
code = "01"
title = "Computer systems"

[[section]]
component = "01"
number = "1.4"
title = "Data types, data structures and algorithms"

[[point]]
component = "01"
section = "1.4"
number = "1.4.3"
title = "Boolean Algebra"

[[point.sub]]
letter = "a"
text = "Define problems using Boolean logic."

[[point.sub]]
letter = "b"
text = "Manipulate Boolean expressions."

[[point]]
component = "01"
section = "1.4"
number = "1.4.4"
title = "A point with no lettered sub-points"
text = "Some text."
"#;

const NOTES: &str = "---\ntitle: Test topic\nspec: \"1.4.3\"\n---\n\n## Heading\n\nSome notes.\n";

const CARDS: &str = r#"
[[card]]
front = "Front"
back = "Back"
spec = "1.4.3"
"#;

const QUIZ: &str = r#"
[[question]]
prompt = "Question?"
options = ["Right", "Wrong"]
answer = 0
explanation = "Because it is right."
spec = "1.4.3"
difficulty = 1
"#;

/// Write a small content folder into a fresh temporary directory, with one
/// file optionally replaced, and return the loader's error message.
fn load_with(name: &str, file: &str, replacement: &str) -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    let topic = root.join("test-topic");
    fs::create_dir_all(&topic).unwrap();

    let files = [
        (root.join("spec.toml"), "spec.toml", SPEC),
        (topic.join("notes.md"), "notes.md", NOTES),
        (topic.join("flashcards.toml"), "flashcards.toml", CARDS),
        (topic.join("quiz.toml"), "quiz.toml", QUIZ),
    ];
    for (path, short_name, default) in files {
        let text = if short_name == file {
            replacement
        } else {
            default
        };
        fs::write(path, text).unwrap();
    }
    content::load(&root).map(|_| ()).map_err(|e| e.to_string())
}

fn assert_error(result: Result<(), String>, file: &str, words: &str) {
    let message = result.expect_err("should fail to load");
    assert!(
        message.contains(file),
        "error should name {file}: {message}"
    );
    assert!(
        message.contains(words),
        "error should mention \"{words}\": {message}"
    );
}

#[test]
fn small_valid_folder_loads() {
    assert_eq!(load_with("valid", "", ""), Ok(()));
}

#[test]
fn answer_index_out_of_range_is_rejected() {
    let quiz = QUIZ.replace("answer = 0", "answer = 2");
    assert_error(
        load_with("answer", "quiz.toml", &quiz),
        "quiz.toml",
        "question 1: answer is 2",
    );
}

#[test]
fn missing_spec_is_rejected() {
    let cards = CARDS.replace("spec = \"1.4.3\"", "");
    assert_error(
        load_with("no-spec", "flashcards.toml", &cards),
        "flashcards.toml",
        "spec",
    );
}

#[test]
fn badly_formed_spec_is_rejected() {
    let quiz = QUIZ.replace("spec = \"1.4.3\"", "spec = \"1.4\"");
    assert_error(
        load_with("bad-spec", "quiz.toml", &quiz),
        "quiz.toml",
        "question 1: spec",
    );
}

#[test]
fn invalid_toml_is_rejected() {
    assert_error(
        load_with(
            "bad-toml",
            "flashcards.toml",
            "[[card]]\nfront = \"unclosed\n",
        ),
        "flashcards.toml",
        "",
    );
}

#[test]
fn misspelt_field_is_rejected() {
    let quiz = QUIZ.replace("answer = 0", "answer = 0\nanwser = 0");
    assert_error(load_with("typo", "quiz.toml", &quiz), "quiz.toml", "anwser");
}

#[test]
fn missing_front_matter_is_rejected() {
    assert_error(
        load_with("no-front", "notes.md", "## Just notes\n"),
        "notes.md",
        "front matter",
    );
}

#[test]
fn bad_difficulty_is_rejected() {
    let quiz = QUIZ.replace("difficulty = 1", "difficulty = 4");
    assert_error(
        load_with("difficulty", "quiz.toml", &quiz),
        "quiz.toml",
        "difficulty",
    );
}

#[test]
fn option_letter_in_explanation_is_rejected() {
    let quiz = QUIZ.replace("Because it is right.", "So option A is right.");
    assert_error(
        load_with("letter", "quiz.toml", &quiz),
        "quiz.toml",
        "option A",
    );
}

// ---------------------------------------------------------------------------
// Spec references must be in spec.toml
// ---------------------------------------------------------------------------

#[test]
fn topic_spec_not_in_spec_toml_is_rejected() {
    let notes = NOTES.replace("1.4.3", "2.1.1");
    assert_error(
        load_with("topic-point", "notes.md", &notes),
        "notes.md",
        "\"2.1.1\" is not a point",
    );
}

#[test]
fn item_spec_not_in_spec_toml_is_rejected() {
    let cards = CARDS.replace("spec = \"1.4.3\"", "spec = \"1.4.9\"");
    assert_error(
        load_with("card-point", "flashcards.toml", &cards),
        "flashcards.toml",
        "card 1: spec \"1.4.9\" is not a point",
    );
}

#[test]
fn known_sub_point_is_accepted() {
    let quiz = QUIZ.replace("spec = \"1.4.3\"", "spec = \"1.4.3(b)\"");
    assert_eq!(load_with("sub-ok", "quiz.toml", &quiz), Ok(()));
}

#[test]
fn unknown_sub_point_is_rejected() {
    let quiz = QUIZ.replace("spec = \"1.4.3\"", "spec = \"1.4.3(c)\"");
    assert_error(
        load_with("sub-letter", "quiz.toml", &quiz),
        "quiz.toml",
        "only has sub-points (a) to (b)",
    );
}

#[test]
fn sub_point_of_point_without_letters_is_rejected() {
    let quiz = QUIZ.replace("spec = \"1.4.3\"", "spec = \"1.4.4(a)\"");
    assert_error(
        load_with("no-letters", "quiz.toml", &quiz),
        "quiz.toml",
        "1.4.4 has no lettered sub-points",
    );
}

// ---------------------------------------------------------------------------
// Mistakes in spec.toml itself
// ---------------------------------------------------------------------------

#[test]
fn spec_point_in_unknown_section_is_rejected() {
    let spec = SPEC.replacen("section = \"1.4\"", "section = \"1.9\"", 1);
    assert_error(
        load_with("spec-section", "spec.toml", &spec),
        "spec.toml",
        "section \"1.9\" is not listed",
    );
}

#[test]
fn spec_sub_letters_out_of_order_are_rejected() {
    let spec = SPEC.replace("letter = \"b\"", "letter = \"c\"");
    assert_error(
        load_with("spec-letters", "spec.toml", &spec),
        "spec.toml",
        "should have letter \"b\"",
    );
}

#[test]
fn spec_point_listed_twice_is_rejected() {
    let spec = SPEC.replace("number = \"1.4.4\"", "number = \"1.4.3\"");
    assert_error(
        load_with("spec-twice", "spec.toml", &spec),
        "spec.toml",
        "point 1.4.3 is listed twice",
    );
}

#[test]
fn spec_point_without_sub_points_or_text_is_rejected() {
    let spec = SPEC.replace("text = \"Some text.\"", "");
    assert_error(
        load_with("spec-empty", "spec.toml", &spec),
        "spec.toml",
        "needs [[point.sub]] entries or a text",
    );
}

#[test]
fn real_spec_toml_has_both_components() {
    let library = content::load(Path::new("content")).unwrap();
    let codes: Vec<&str> = library
        .spec
        .components
        .iter()
        .map(|c| c.code.as_str())
        .collect();
    assert_eq!(codes, ["01", "02"]);
    let point = library.spec.point("1.1.1").expect("1.1.1 should be listed");
    assert_eq!(point.subs.len(), 5, "1.1.1 has sub-points (a) to (e)");
}

#[test]
fn windows_line_endings_are_accepted() {
    assert_eq!(
        load_with("crlf", "notes.md", &NOTES.replace('\n', "\r\n")),
        Ok(())
    );
}
