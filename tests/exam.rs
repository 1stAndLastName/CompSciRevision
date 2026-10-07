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
