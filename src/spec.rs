//! The OCR H446 specification outline, loaded from `content/spec.toml`.
//!
//! The file lists components, sections and spec points as flat lists (easy to
//! read and edit). `load` checks them and nests them (component > section >
//! point) for the home page. `check_reference` is used by the content loader
//! so that every `spec` value in content/ names a real spec point.

use crate::content::{ContentError, parse_toml, problem};
use serde::Deserialize;
use std::path::Path;

/// The whole outline, nested for display.
pub struct Spec {
    pub components: Vec<Component>,
}

pub struct Component {
    pub code: String,
    pub title: String,
    pub sections: Vec<Section>,
}

pub struct Section {
    pub number: String,
    pub title: String,
    pub points: Vec<Point>,
}

/// One spec point such as 1.4.3, exactly as it is in `spec.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub component: String,
    pub section: String,
    pub number: String,
    pub title: String,
    /// Only for points without lettered sub-points (1.5.2).
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default, rename = "sub")]
    pub subs: Vec<SubPoint>,
}

/// One lettered sub-point such as 1.4.3(b).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubPoint {
    pub letter: String,
    pub text: String,
    #[serde(default)]
    pub items: Vec<String>,
}

// What is in the file: three flat lists.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpecFile {
    component: Vec<RawComponent>,
    section: Vec<RawSection>,
    point: Vec<Point>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawComponent {
    code: String,
    title: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSection {
    component: String,
    number: String,
    title: String,
}

/// Load `spec.toml`, check it, and nest points inside sections inside components.
pub fn load(path: &Path) -> Result<Spec, ContentError> {
    let file: SpecFile = parse_toml(path)?;
    check(&file).map_err(|message| problem(path, message))?;

    // Put each section inside its component, then each point inside its
    // section. `check` has already made sure every one has somewhere to go.
    let mut components: Vec<Component> = file
        .component
        .into_iter()
        .map(|raw| Component {
            code: raw.code,
            title: raw.title,
            sections: Vec::new(),
        })
        .collect();
    for raw in file.section {
        if let Some(component) = components.iter_mut().find(|c| c.code == raw.component) {
            component.sections.push(Section {
                number: raw.number,
                title: raw.title,
                points: Vec::new(),
            });
        }
    }
    for point in file.point {
        let section = components
            .iter_mut()
            .flat_map(|c| c.sections.iter_mut())
            .find(|s| s.number == point.section);
        if let Some(section) = section {
            section.points.push(point);
        }
    }

    Ok(Spec { components })
}

/// Check the flat lists fit together. Returns a message naming the problem.
fn check(file: &SpecFile) -> Result<(), String> {
    let mut seen_codes: Vec<&str> = Vec::new();
    for component in &file.component {
        if seen_codes.contains(&component.code.as_str()) {
            return Err(format!("component {} is listed twice", component.code));
        }
        seen_codes.push(&component.code);
    }

    let mut seen_sections: Vec<&str> = Vec::new();
    for section in &file.section {
        let name = format!("section {}", section.number);
        if !seen_codes.contains(&section.component.as_str()) {
            return Err(format!(
                "{name}: component \"{}\" is not listed",
                section.component
            ));
        }
        if !is_numbers(&section.number, 2) {
            return Err(format!("{name}: number should look like \"1.4\""));
        }
        // Section 1.x belongs to component 01, 2.x to 02.
        let first = section.number.split('.').next().unwrap_or_default();
        if first != section.component.trim_start_matches('0') {
            return Err(format!(
                "{name}: should not be in component {}",
                section.component
            ));
        }
        if seen_sections.contains(&section.number.as_str()) {
            return Err(format!("{name} is listed twice"));
        }
        seen_sections.push(&section.number);
    }

    let mut seen_points: Vec<&str> = Vec::new();
    for point in &file.point {
        let name = format!("point {}", point.number);
        if !is_numbers(&point.number, 3) {
            return Err(format!("{name}: number should look like \"1.4.3\""));
        }
        let section = file.section.iter().find(|s| s.number == point.section);
        match section {
            None => {
                return Err(format!(
                    "{name}: section \"{}\" is not listed",
                    point.section
                ));
            }
            Some(s) if s.component != point.component => {
                return Err(format!(
                    "{name}: component is \"{}\" but section {} is in component {}",
                    point.component, s.number, s.component
                ));
            }
            Some(_) => {}
        }
        if !point.number.starts_with(&format!("{}.", point.section)) {
            return Err(format!(
                "{name}: number does not start with its section {}",
                point.section
            ));
        }
        if seen_points.contains(&point.number.as_str()) {
            return Err(format!("{name} is listed twice"));
        }
        seen_points.push(&point.number);
        if point.title.trim().is_empty() {
            return Err(format!("{name}: title is empty"));
        }
        if point.subs.is_empty() && point.text.is_none() {
            return Err(format!("{name}: needs [[point.sub]] entries or a text"));
        }

        // Letters must run a, b, c, ... so a missing or repeated one is caught.
        for (i, sub) in point.subs.iter().enumerate() {
            let expected = char::from(b'a' + i as u8).to_string();
            if sub.letter != expected {
                return Err(format!(
                    "{name}: sub-point {} should have letter \"{expected}\" but has \"{}\"",
                    i + 1,
                    sub.letter
                ));
            }
            if sub.text.trim().is_empty() {
                return Err(format!("{name}({}): text is empty", sub.letter));
            }
        }
    }

    for section in &file.section {
        if !file.point.iter().any(|p| p.section == section.number) {
            return Err(format!("section {} has no points", section.number));
        }
    }
    Ok(())
}

impl Spec {
    pub fn points(&self) -> impl Iterator<Item = &Point> {
        self.components
            .iter()
            .flat_map(|c| &c.sections)
            .flat_map(|s| &s.points)
    }

    pub fn point(&self, number: &str) -> Option<&Point> {
        self.points().find(|p| p.number == number)
    }

    /// Check that a reference such as "1.4.3" or "1.4.3(b)" is in the spec.
    /// The error message says what is wrong, for the content loader to report.
    pub fn check_reference(&self, reference: &str) -> Result<(), String> {
        let (number, letter) = match reference.split_once('(') {
            Some((number, rest)) => (number, Some(rest.trim_end_matches(')'))),
            None => (reference, None),
        };
        let Some(point) = self.point(number) else {
            return Err(format!(
                "spec \"{reference}\" is not a point in content/spec.toml"
            ));
        };
        let Some(letter) = letter else {
            return Ok(());
        };
        if point.subs.is_empty() {
            return Err(format!(
                "spec \"{reference}\": {number} has no lettered sub-points, so use \"{number}\""
            ));
        }
        if point.subs.iter().any(|s| s.letter == letter) {
            Ok(())
        } else {
            let last = &point.subs[point.subs.len() - 1].letter;
            Err(format!(
                "spec \"{reference}\": {number} only has sub-points (a) to ({last})"
            ))
        }
    }
}

/// True for `count` dot-separated numbers, e.g. "1.4" (2) or "1.4.3" (3).
fn is_numbers(text: &str, count: usize) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == count
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}
