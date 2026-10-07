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
//!
//! Students mark themselves: each bullet point in a mark scheme row becomes a
//! "mark point" they can tick. Some parts have one right answer (a conversion,
//! a value a program prints, a Boolean expression); their answers are in
//! `exam-questions/answers/<spec point>.toml`, and exam.js marks those parts
//! in the browser.

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
    /// The answer boxes, if this part is marked automatically.
    pub auto: Option<Vec<AnswerBox>>,
}

pub struct MarkRow {
    pub label: String,
    pub marks: Option<u32>,
    /// The answer column, split into plain text and runs of bullet points.
    pub answer: Vec<AnswerBlock>,
    /// The mark scheme's guidance column, including examiners' comments.
    pub guidance_html: Option<String>,
}

/// Part of a mark scheme answer: some text (or a table, or code), or a run of
/// bullet points, each one a mark point students can tick.
pub enum AnswerBlock {
    Text(String),
    Points(Vec<MarkPoint>),
}

pub struct MarkPoint {
    /// Unique on the page, e.g. "q3-m1-2" (question 3, row 1, point 2).
    pub id: String,
    /// What exam.js saves when it is ticked, e.g. "m1-2".
    pub key: String,
    pub html: String,
}

/// One box a student types an answer into, on a part that is marked automatically.
pub struct AnswerBox {
    /// Shown before the box, e.g. "Output for 10". None means "Your answer".
    pub label: Option<String>,
    pub kind: AnswerKind,
    /// The accepted answers, one per line (for an expression, the one answer).
    pub accept: String,
    /// For an expression: it must also have no more operators than `accept`.
    pub simplest: bool,
    pub marks: u32,
}

/// How exam.js compares what a student types with the accepted answers.
#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AnswerKind {
    /// Values: spaces, commas and case are ignored, so "1001 0101" matches "10010101".
    Exact,
    /// Words: case and punctuation are ignored, so "Real-time" matches "real time".
    Words,
    /// A Boolean expression: any expression with the same truth table is right.
    Expression,
}

impl AnswerKind {
    /// The name exam.js reads from the page.
    pub fn name(&self) -> &'static str {
        match self {
            AnswerKind::Exact => "exact",
            AnswerKind::Words => "words",
            AnswerKind::Expression => "expression",
        }
    }
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

/// `exam-questions/answers/<spec point>.toml`: answers for automatic marking.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerFile {
    spec: String,
    #[serde(default)]
    part: Vec<RawAutoPart>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAutoPart {
    label: String,
    // `box` is a Rust keyword, so the field has another name in Rust.
    #[serde(rename = "box")]
    boxes: Vec<RawBox>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBox {
    label: Option<String>,
    #[serde(rename = "type")]
    kind: AnswerKind,
    #[serde(default)]
    accept: Vec<String>,
    answer: Option<String>,
    #[serde(default)]
    simplest: bool,
    marks: Option<u32>,
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
    load_answers(&dir.join("answers"), &mut topics)?;
    Ok(topics)
}

/// Load `answers/<spec point>.toml` (if the folder exists) and attach each
/// part's answer boxes to the part, checking every one against the questions.
fn load_answers(dir: &Path, topics: &mut [ExamTopic]) -> Result<(), ContentError> {
    if !dir.exists() {
        return Ok(());
    }
    let entries =
        fs::read_dir(dir).map_err(|e| problem(dir, format!("cannot read folder: {e}")))?;
    for entry in entries {
        let path = entry
            .map_err(|e| problem(dir, format!("cannot read folder: {e}")))?
            .path();
        if path.extension().is_none_or(|ext| ext != "toml") {
            continue;
        }
        let file: AnswerFile = parse_toml(&path)?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if file.spec != stem {
            return Err(problem(
                &path,
                format!(
                    "spec is \"{}\" but the file is named {stem}.toml; they must match",
                    file.spec
                ),
            ));
        }
        let topic = topics
            .iter_mut()
            .find(|t| t.spec == file.spec)
            .ok_or_else(|| {
                problem(
                    &path,
                    format!("there are no exam questions for {}", file.spec),
                )
            })?;
        for raw in file.part {
            let part = topic
                .questions
                .iter_mut()
                .flat_map(|q| q.parts.iter_mut())
                .find(|p| p.label == raw.label)
                .ok_or_else(|| problem(&path, format!("there is no part {}", raw.label)))?;
            let boxes = check_boxes(&raw, part).map_err(|message| problem(&path, message))?;
            if part.auto.replace(boxes).is_some() {
                return Err(problem(
                    &path,
                    format!("part {} is listed twice", raw.label),
                ));
            }
        }
    }
    Ok(())
}

/// Check one part's answer boxes and work out each box's marks.
fn check_boxes(raw: &RawAutoPart, part: &ExamPart) -> Result<Vec<AnswerBox>, String> {
    let label = &raw.label;
    let Some(part_marks) = part.marks else {
        return Err(format!(
            "part {label} has no marks, so it cannot be marked automatically"
        ));
    };
    if raw.boxes.is_empty() {
        return Err(format!("part {label} has no answer boxes (`box = [...]`)"));
    }
    let mut boxes = Vec::new();
    for raw_box in &raw.boxes {
        // One box is worth the whole part; several are worth 1 mark each unless they say.
        let marks = raw_box
            .marks
            .unwrap_or(if raw.boxes.len() == 1 { part_marks } else { 1 });
        let accept = match (raw_box.kind, &raw_box.answer) {
            (AnswerKind::Expression, Some(answer)) if raw_box.accept.is_empty() => {
                check_expression(answer).map_err(|e| format!("part {label}: {e}"))?;
                answer.clone()
            }
            (AnswerKind::Expression, _) => {
                return Err(format!(
                    "part {label}: an expression box needs `answer` (one expression), not `accept`"
                ));
            }
            (_, None) if !raw_box.accept.is_empty() => {
                if raw_box
                    .accept
                    .iter()
                    .any(|a| a.trim().is_empty() || a.contains('\n'))
                {
                    return Err(format!(
                        "part {label}: an accepted answer is empty or has a line break"
                    ));
                }
                raw_box.accept.join("\n")
            }
            _ => {
                return Err(format!(
                    "part {label}: an {} box needs `accept` (a list of answers), not `answer`",
                    raw_box.kind.name()
                ));
            }
        };
        if raw_box.simplest && raw_box.kind != AnswerKind::Expression {
            return Err(format!(
                "part {label}: `simplest` only applies to expression boxes"
            ));
        }
        boxes.push(AnswerBox {
            label: raw_box.label.clone(),
            kind: raw_box.kind,
            accept,
            simplest: raw_box.simplest,
            marks,
        });
    }
    let total: u32 = boxes.iter().map(|b| b.marks).sum();
    if total != part_marks {
        return Err(format!(
            "part {label}: the boxes are worth {total} marks but the part is worth {part_marks}"
        ));
    }
    Ok(boxes)
}

/// A rough check that an answer is a Boolean expression in OCR notation:
/// capital letters, 0, 1, `∧ ∨ ¬ ⊻ ≡`, brackets that pair up, and spaces.
/// (exam.js does the real parsing; this catches typing slips such as "v" for "∨".)
fn check_expression(expression: &str) -> Result<(), String> {
    let mut depth = 0;
    for c in expression.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Err(format!("\"{expression}\" has a ) with no (")),
            ')' => depth -= 1,
            'A'..='Z' | '0' | '1' | '∧' | '∨' | '¬' | '⊻' | '≡' | ' ' => {}
            other => {
                return Err(format!(
                    "\"{expression}\" has '{other}'; use capital letters and ∧ ∨ ¬ ⊻ ≡"
                ));
            }
        }
    }
    if depth != 0 {
        return Err(format!("\"{expression}\" has a ( with no )"));
    }
    if !expression
        .chars()
        .any(|c| c.is_ascii_uppercase() || c == '0' || c == '1')
    {
        return Err("an expression box's answer is empty".to_string());
    }
    Ok(())
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
            auto: None,
        })
        .collect();
    let mut other_mark_scheme = Vec::new();
    for (index, row) in raw.mark_scheme.into_iter().enumerate() {
        let row = MarkRow {
            label: row.label,
            marks: row.marks,
            answer: answer_blocks(&row.answer, raw.number, index),
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

/// Split a mark scheme answer into text and runs of mark points. Each bullet
/// point that starts a line ("- ..." or "* ..." or "• ...") is a mark point,
/// together with any lines indented under it. Bullets inside a code block
/// are code, not mark points.
fn answer_blocks(markdown: &str, question: u32, row: usize) -> Vec<AnswerBlock> {
    // First split the lines into pieces: (is this a point?, its Markdown).
    let mut pieces: Vec<(bool, String)> = Vec::new();
    let mut in_code = false;
    let mut after_blank = false;
    for line in markdown.lines() {
        let bullet = ["- ", "* ", "• "]
            .iter()
            .find_map(|marker| line.strip_prefix(marker));
        let in_point = pieces.last().is_some_and(|(is_point, _)| *is_point);
        if !in_code && let Some(rest) = bullet {
            pieces.push((true, format!("{rest}\n")));
        } else if in_point
            && !in_code
            && after_blank
            && !line.trim().is_empty()
            && !line.starts_with("  ")
        {
            // Not indented after a blank line: the point has ended.
            pieces.push((false, format!("{line}\n")));
        } else {
            // Lines inside a point lose the 2-space indent that put them there,
            // so a nested list stays a list rather than becoming code.
            let line = if in_point {
                line.strip_prefix("  ").unwrap_or(line)
            } else {
                line
            };
            match pieces.last_mut() {
                Some((_, text)) => {
                    text.push_str(line);
                    text.push('\n');
                }
                None => pieces.push((false, format!("{line}\n"))),
            }
        }
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        after_blank = line.trim().is_empty();
    }

    // Then render them, grouping points that follow each other into one list.
    let mut blocks: Vec<AnswerBlock> = Vec::new();
    let mut count = 0;
    for (is_point, text) in pieces {
        if text.trim().is_empty() {
            continue;
        }
        if !is_point {
            blocks.push(AnswerBlock::Text(markdown_to_html(&text)));
            continue;
        }
        let point = MarkPoint {
            id: format!("q{question}-m{row}-{count}"),
            key: format!("m{row}-{count}"),
            html: markdown_to_html(&text),
        };
        count += 1;
        match blocks.last_mut() {
            Some(AnswerBlock::Points(points)) => points.push(point),
            _ => blocks.push(AnswerBlock::Points(vec![point])),
        }
    }
    blocks
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
    fn bullets_become_mark_points() {
        let blocks = answer_blocks(
            "1 mark each:\n\n- Clock speed\n- Number of cores\n  - dual, quad\n\nNot cache size\n\n```\n- code\n```",
            3,
            1,
        );
        // The text, the points, then the rest (text and code) as one block.
        assert_eq!(blocks.len(), 3);
        let AnswerBlock::Points(points) = &blocks[1] else {
            panic!("the bullets should be points");
        };
        assert_eq!(points.len(), 2);
        assert_eq!(points[1].id, "q3-m1-1");
        assert!(points[1].html.contains("<li>dual, quad</li>"));
        let AnswerBlock::Text(rest) = &blocks[2] else {
            panic!("the text after the bullets should be text");
        };
        assert!(rest.contains("Not cache size"));
        assert!(rest.contains("- code"), "a bullet inside code is code");
    }

    #[test]
    fn expressions_must_use_ocr_notation() {
        assert!(check_expression("¬(A ∧ B) ⊻ C").is_ok());
        assert!(check_expression("A v B").is_err());
        assert!(check_expression("(A ∧ B").is_err());
        assert!(check_expression("A ∧ B)").is_err());
    }

    #[test]
    fn part_labels_start_with_the_question_number() {
        assert!(starts_with_number("1(a)", 1));
        assert!(starts_with_number("10(a)(ii)", 10));
        assert!(!starts_with_number("10(a)", 1));
        assert!(!starts_with_number("(a)", 1));
    }
}
