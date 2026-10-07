//! The exam questions in exam-questions/ load, and the loader rejects broken files.

use revision_site::{content, exam, load_site};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn exam_questions_load() {
    let library = load_site().expect("content/ and exam-questions/ should load");
    assert!(!library.exam.is_empty());
    for topic in &library.exam {
        assert!(
            !topic.questions.is_empty(),
            "{} has no questions",
            topic.spec
        );
        for question in &topic.questions {
            assert!(
                question.marks > 0,
                "{} question {}",
                topic.spec,
                question.number
            );
        }
    }
}

/// Every diagram redrawn by `tools/redraw-exam-diagrams` says it was made by
/// an AI model, just before the drawing, and the Mermaid it needs is there.
#[test]
fn redrawn_diagrams_are_labelled_as_ai_made() {
    assert!(Path::new("static/vendor/mermaid-12.0.0.min.js").exists());
    for entry in fs::read_dir("exam-questions").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "toml") {
            continue; // the answers/ folder
        }
        let text = fs::read_to_string(&path).unwrap();
        // `split` cuts the file at every Mermaid block; each piece before one
        // must end with the label (plus a little space for a line or two).
        let pieces: Vec<&str> = text.split("```mermaid").collect();
        for before in &pieces[..pieces.len() - 1] {
            let tail: String = before.chars().rev().take(200).collect();
            let tail: String = tail.chars().rev().collect();
            assert!(
                tail.contains("by an AI model"),
                "{}: a Mermaid diagram has no AI label before it",
                path.display()
            );
        }
    }
}

const GOOD: &str = r#"
spec = "1.4.3"

[[question]]
number = 1
marks = 3

[[question.part]]
label = "1(a)"
marks = 3
text = "Simplify `¬¬A`."

[[question.mark_scheme]]
label = "1(a)"
marks = 3
answer = "`A`"
"#;

/// Load a folder holding one exam file, and return the loader's error message.
fn load_file(name: &str, file_name: &str, text: &str) -> Result<(), String> {
    let spec = content::load(Path::new("content")).unwrap().spec;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(file_name), text).unwrap();
    exam::load(&dir, &spec)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[test]
fn a_good_file_loads_with_its_mark_scheme_under_the_part() {
    let spec = content::load(Path::new("content")).unwrap().spec;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("exam-good");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("1.4.3.toml"), GOOD).unwrap();
    let topics = exam::load(&dir, &spec).unwrap();
    let part = &topics[0].questions[0].parts[0];
    assert_eq!(part.mark_scheme.len(), 1);
    assert!(part.text_html.contains("<code>¬¬A</code>"));
    assert_eq!(topics[0].title, "Boolean Algebra");
}

#[test]
fn the_file_name_must_match_its_spec_point() {
    let message = load_file("exam-name", "1.4.2.toml", GOOD).unwrap_err();
    assert!(message.contains("must match"), "{message}");
}

#[test]
fn unknown_spec_points_are_rejected() {
    let text = GOOD.replace("1.4.3", "9.9.9");
    let message = load_file("exam-unknown", "9.9.9.toml", &text).unwrap_err();
    assert!(message.contains("not in spec.toml"), "{message}");
}

#[test]
fn part_labels_must_start_with_the_question_number() {
    let text = GOOD.replace(
        "label = \"1(a)\"\nmarks = 3\ntext",
        "label = \"2(a)\"\nmarks = 3\ntext",
    );
    let message = load_file("exam-label", "1.4.3.toml", &text).unwrap_err();
    assert!(message.contains("should start with 1"), "{message}");
}

#[test]
fn typos_in_field_names_are_rejected() {
    let text = GOOD.replace("answer =", "anwser =");
    let message = load_file("exam-typo", "1.4.3.toml", &text).unwrap_err();
    assert!(message.contains("anwser"), "{message}");
}

/// Load GOOD (question 1, part 1(a), 3 marks) with this answers file, and
/// return the loader's error message, or the part's number of answer boxes.
fn load_with_answers(name: &str, answers: &str) -> Result<usize, String> {
    let spec = content::load(Path::new("content")).unwrap().spec;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("answers")).unwrap();
    fs::write(dir.join("1.4.3.toml"), GOOD).unwrap();
    fs::write(dir.join("answers").join("1.4.3.toml"), answers).unwrap();
    let topics = exam::load(&dir, &spec).map_err(|e| e.to_string())?;
    Ok(topics[0].questions[0].parts[0]
        .auto
        .as_ref()
        .map_or(0, |boxes| boxes.len()))
}

#[test]
fn answers_attach_to_their_part() {
    let boxes = load_with_answers(
        "answers-good",
        r#"spec = "1.4.3"
[[part]]
label = "1(a)"
box = [{ type = "expression", answer = "A", simplest = true }]"#,
    );
    assert_eq!(boxes, Ok(1));
}

#[test]
fn answers_for_a_missing_part_are_rejected() {
    let message = load_with_answers(
        "answers-no-part",
        r#"spec = "1.4.3"
[[part]]
label = "1(z)"
box = [{ type = "exact", accept = ["5"] }]"#,
    )
    .unwrap_err();
    assert!(message.contains("no part 1(z)"), "{message}");
}

#[test]
fn answer_boxes_must_add_up_to_the_part() {
    // Two boxes are worth 1 mark each, but the part is worth 3.
    let message = load_with_answers(
        "answers-marks",
        r#"spec = "1.4.3"
[[part]]
label = "1(a)"
box = [{ type = "exact", accept = ["1"] }, { type = "exact", accept = ["2"] }]"#,
    )
    .unwrap_err();
    assert!(message.contains("worth 2 marks"), "{message}");
}

#[test]
fn expression_answers_use_ocr_notation() {
    let message = load_with_answers(
        "answers-notation",
        r#"spec = "1.4.3"
[[part]]
label = "1(a)"
box = [{ type = "expression", answer = "A v B" }]"#,
    )
    .unwrap_err();
    assert!(message.contains("'v'"), "{message}");
}
