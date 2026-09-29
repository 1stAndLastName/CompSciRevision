//! Real OCR exam questions and their mark schemes.
//!
//! `tools/extract-exam-questions` copies the text of the past-paper questions
//! from the PDFs in `sources/` (which is never committed) into one file per
//! spec point, `exam-questions/<spec point>.toml`. This module loads and checks
//! those files once at startup, like `content.rs` does for `content/`.
//!
//! A question has parts ("1(a)", "1(b)(i)", ...) and mark scheme rows, each
//! with its own label. Each row is shown under the part it belongs to: the
//! part with the longest label that the row's label starts with.

use crate::content::{ContentError, markdown_to_html, parse_toml, problem};
use crate::spec::Spec;
use serde::Deserialize;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// What the rest of the site uses
// ---------------------------------------------------------------------------

/// The exam questions for one spec point.
pub struct ExamTopic {
    /// The spec point, e.g. "1.4.3".
    pub spec: String,
    /// The spec point's title from spec.toml.
    pub title: String,
    pub questions: Vec<ExamQuestion>,
}

pub struct ExamQuestion {
    pub number: u32,
    pub marks: u32,
    /// True if the paper had a diagram that is not shown here.
    pub diagram: bool,
    pub parts: Vec<ExamPart>,
    /// Mark scheme rows that match none of the parts, shown after the last part.
    pub other_mark_scheme: Vec<MarkRow>,
}

pub struct ExamPart {
    pub label: String,
    /// The marks printed for this part, or else the marks its mark scheme rows add up to.
    pub marks: Option<u32>,
    pub text_html: String,
    pub mark_scheme: Vec<MarkRow>,
}

pub struct MarkRow {
    pub label: String,
    pub marks: Option<u32>,
    pub answer_html: String,
    /// The mark scheme's guidance column, including examiners' comments.
    pub guidance_html: Option<String>,
}

/// "1 mark" or "3 marks".
pub fn marks_text(marks: u32) -> String {
    if marks == 1 {
        "1 mark".to_string()
    } else {
        format!("{marks} marks")
    }
}

impl ExamQuestion {
    pub fn marks_text(&self) -> String {
        marks_text(self.marks)
    }
}

impl ExamPart {
    /// "3 marks", for screen readers after the printed "[3]".
    pub fn marks_text(&self) -> Option<String> {
        self.marks.map(marks_text)
    }
}

impl MarkRow {
    pub fn marks_text(&self) -> Option<String> {
        self.marks.map(marks_text)
    }
}

impl ExamTopic {
    /// Total marks across every question.
    pub fn marks(&self) -> u32 {
        self.questions.iter().map(|q| q.marks).sum()
    }

    /// How many questions need a diagram that is not shown.
    pub fn with_diagrams(&self) -> usize {
        self.questions.iter().filter(|q| q.diagram).count()
    }
}

// ---------------------------------------------------------------------------
// What is in the files
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExamFile {
    spec: String,
    #[serde(default)]
    question: Vec<RawQuestion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawQuestion {
    number: u32,
    marks: u32,
    #[serde(default)]
    diagram: bool,
    #[serde(default)]
    part: Vec<RawPart>,
    #[serde(default)]
    mark_scheme: Vec<RawMarkRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPart {
    label: String,
    marks: Option<u32>,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMarkRow {
    label: String,
    marks: Option<u32>,
    answer: String,
    guidance: Option<String>,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load every `<spec point>.toml` in `dir`, sorted by spec point. A missing
/// folder just means there are no exam questions.
pub fn load(dir: &Path, spec: &Spec) -> Result<Vec<ExamTopic>, ContentError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries =
        fs::read_dir(dir).map_err(|e| problem(dir, format!("cannot read folder: {e}")))?;
    let mut topics = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| problem(dir, format!("cannot read folder: {e}")))?
            .path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            topics.push(load_file(&path, spec)?);
        }
    }
    // Sort by the numbers in the spec point, so 2.1.10 would come after 2.1.9.
    topics.sort_by_key(|topic| {
        topic
            .spec
            .split('.')
            .map(|n| n.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    });
    Ok(topics)
}

fn load_file(path: &Path, spec: &Spec) -> Result<ExamTopic, ContentError> {
    let file: ExamFile = parse_toml(path)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if file.spec != stem {
        return Err(problem(
            path,
            format!(
                "spec is \"{}\" but the file is named {stem}.toml; they must match",
                file.spec
            ),
        ));
    }
    let point = spec
        .point(&file.spec)
        .ok_or_else(|| problem(path, format!("spec \"{}\" is not in spec.toml", file.spec)))?;

    let mut questions = Vec::new();
    let mut last_number = 0;
    for raw in file.question {
        let label = format!("question {}", raw.number);
        if raw.number <= last_number {
            return Err(problem(
                path,
                format!("{label}: question numbers must go up in order"),
            ));
        }
        last_number = raw.number;
        if raw.part.is_empty() {
            return Err(problem(path, format!("{label}: has no [[question.part]]")));
        }
        for part in &raw.part {
            if !starts_with_number(&part.label, raw.number) {
                return Err(problem(
                    path,
                    format!(
                        "{label}: part label \"{}\" should start with {}",
                        part.label, raw.number
                    ),
                ));
            }
            if part.text.trim().is_empty() && part.marks.is_none() {
                return Err(problem(
                    path,
                    format!("{label}: part {} has no text", part.label),
                ));
            }
        }
        questions.push(build_question(raw));
    }

    Ok(ExamTopic {
        spec: file.spec,
        title: point.title.clone(),
        questions,
    })
}

/// "10(a)" starts with question 10, "1(a)" does not.
fn starts_with_number(label: &str, number: u32) -> bool {
    let digits: String = label.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits == number.to_string()
}

/// Render the Markdown and put each mark scheme row under its part.
fn build_question(raw: RawQuestion) -> ExamQuestion {
    let labels: Vec<String> = raw.part.iter().map(|p| p.label.clone()).collect();
    let mut parts: Vec<ExamPart> = raw
        .part
        .into_iter()
        .map(|p| ExamPart {
            label: p.label,
            marks: p.marks,
            text_html: markdown_to_html(&p.text),
            mark_scheme: Vec::new(),
        })
        .collect();
    let mut other_mark_scheme = Vec::new();
    for row in raw.mark_scheme {
        let row = MarkRow {
            label: row.label,
            marks: row.marks,
            answer_html: markdown_to_html(&row.answer),
            guidance_html: row
                .guidance
                .filter(|g| !g.trim().is_empty())
                .map(|g| markdown_to_html(&g)),
        };
        match part_for(&labels, &row.label) {
            Some(index) => parts[index].mark_scheme.push(row),
            None => other_mark_scheme.push(row),
        }
    }
    // A part with no printed marks (lost from the PDF) gets its mark scheme's marks.
    for part in &mut parts {
        if part.marks.is_none() {
            let from_rows: Vec<u32> = part.mark_scheme.iter().filter_map(|r| r.marks).collect();
            if !from_rows.is_empty() {
                part.marks = Some(from_rows.iter().sum());
            }
        }
    }
    ExamQuestion {
        number: raw.number,
        marks: raw.marks,
        diagram: raw.diagram,
        parts,
        other_mark_scheme,
    }
}

/// Which part a mark scheme row belongs to: the part with the longest label
/// that the row's label starts with ("1(b)(ii)" goes under "1(b)(ii)", or
/// "1(b)" if there is no "1(b)(ii)"). If there is none, the first part whose
/// label starts with the row's label (a row for "5(c)" goes under "5(c)(i)").
pub fn part_for(labels: &[String], row_label: &str) -> Option<usize> {
    labels
        .iter()
        .enumerate()
        .filter(|(_, label)| row_label.starts_with(label.as_str()))
        .max_by_key(|(_, label)| label.len())
        .map(|(index, _)| index)
        .or_else(|| labels.iter().position(|label| label.starts_with(row_label)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rows_go_under_the_closest_part() {
        let parts = labels(&["1(a)", "1(b)", "1(b)(i)", "1(b)(ii)"]);
        assert_eq!(part_for(&parts, "1(a)"), Some(0));
        assert_eq!(part_for(&parts, "1(b)(ii)"), Some(3));
        assert_eq!(part_for(&parts, "1(b)(iii)"), Some(1));
        assert_eq!(part_for(&parts, "1(c)"), None);
        let parts = labels(&["5(c)(i)"]);
        assert_eq!(part_for(&parts, "5(c)"), Some(0));
    }

    #[test]
    fn part_labels_start_with_the_question_number() {
        assert!(starts_with_number("1(a)", 1));
        assert!(starts_with_number("10(a)(ii)", 10));
        assert!(!starts_with_number("10(a)", 1));
        assert!(!starts_with_number("(a)", 1));
    }
}
