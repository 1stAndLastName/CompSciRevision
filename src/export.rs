//! Export the whole site as plain files, for GitHub Pages
//! (`cargo run --bin export -- --base /CompSciRevision --out dist`).
//!
//! Each page becomes an `index.html` in its own folder, so
//! `/topics/boolean-algebra/quiz` is `topics/boolean-algebra/quiz/index.html`.
//! `static/` is copied as it is, and `404.html` is the page GitHub Pages
//! shows for addresses that don't exist.

use crate::content::Library;
use crate::{STATIC_DIR, routes};
use std::fs;
use std::io;
use std::path::Path;

/// Written into the output folder, so a later export knows it may clear it.
const MARKER: &str = ".site-export";

/// Write every page into `out`. Returns the number of HTML pages written.
pub fn export_site(library: &Library, base: &str, out: &Path) -> io::Result<usize> {
    // Only clear a folder this function made before (it has the marker file),
    // so a mistyped --out can never delete something else.
    if out.exists() {
        let is_empty = fs::read_dir(out)?.next().is_none();
        if !is_empty && !out.join(MARKER).exists() {
            return Err(io::Error::other(format!(
                "{} is not empty and was not made by the export; choose another folder",
                out.display()
            )));
        }
        fs::remove_dir_all(out)?;
    }
    fs::create_dir_all(out)?;
    fs::write(
        out.join(MARKER),
        "Made by `cargo run --bin export`. Safe to delete.\n",
    )?;
    // Tell GitHub Pages not to run Jekyll over the files.
    fs::write(out.join(".nojekyll"), "")?;

    let mut pages = 0;
    let mut write = |path: &str, html: askama::Result<String>| -> io::Result<()> {
        let file = out.join(path);
        fs::create_dir_all(file.parent().expect("page paths have a folder"))?;
        fs::write(file, html.map_err(io::Error::other)?)?;
        pages += 1;
        Ok(())
    };

    write("index.html", routes::render_home(library, base))?;
    for topic in &library.topics {
        let folder = format!("topics/{}", topic.slug);
        write(
            &format!("{folder}/index.html"),
            routes::render_topic(library, topic, base),
        )?;
        write(
            &format!("{folder}/flashcards/index.html"),
            routes::render_flashcards(topic, base),
        )?;
        write(
            &format!("{folder}/quiz/index.html"),
            routes::render_quiz(topic, base),
        )?;
    }
    write("exam/index.html", routes::render_exam_index(library, base))?;
    for exam in &library.exam {
        write(
            &format!("exam/{}/index.html", exam.spec),
            routes::render_exam(library, exam, base),
        )?;
    }
    write("404.html", routes::render_not_found(base))?;

    copy_folder(Path::new(STATIC_DIR), &out.join("static"))?;
    Ok(pages)
}

/// Copy a folder and everything in it.
fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_folder(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
