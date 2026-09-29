"""Turn everything in sources/ into searchable Markdown.

Run it with `tools/convert-sources`. It works in five stages:

1. catalogue  every file in sources/ must have an entry in sources/manifest.toml.
              New files get a guessed entry marked `review = "yes"` for you to check.
2. convert    each file becomes one Markdown file in sources/.work/raw/, with
              <!-- page N --> markers. Only new or changed files are converted
              (a SHA-256 of each file is kept in sources/.work/cache.json).
3. split      the raw Markdown is cleaned (page headers and footers, URLs and
              personal details removed) and split into one file per section in
              sources/text/<type>/<source>/<nn>-<slug>.md. Exam papers become
              one file per question, with the mark scheme alongside.
4. index      sources/index.md: one line per section file with its spec points.
5. check      fails if a URL or anything in sources/redact.toml is left.

Stages 3-5 are quick and always run; stage 2 is the slow one (GPU OCR).
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ROOT / "sources"
WORK = SOURCES / ".work"
RAW = WORK / "raw"
IMAGES = WORK / "images"
PAGES = WORK / "pages"
DIAGRAMS = WORK / "diagrams"
SECOND_OCR = WORK / "tesseract"
WORDLISTS = [Path("/usr/share/dict/british-english"), Path("/usr/share/dict/american-english")]
CACHE_FILE = WORK / "cache.json"
TEXT = SOURCES / "text"
MANIFEST = SOURCES / "manifest.toml"
REDACT = SOURCES / "redact.toml"
INDEX = SOURCES / "index.md"
MAPS = Path(__file__).with_name("maps.toml")
SPEC_TOML = ROOT / "content" / "spec.toml"

# Bump this when the conversion code changes, to convert everything again.
CONVERTER_VERSION = "1"

# Files and folders in sources/ that this tool makes itself.
GENERATED = {".work", "text", "manifest.toml", "index.md", "redact.toml"}

TRUST_BY_TYPE = {
    "spec": "authoritative",
    "textbook": "primary",
    "web-notes": "primary",
    "exam-questions": "exam-reference",
    "student-notes": "low",
}


def say(message: str) -> None:
    print(message, flush=True)


def slugify(text: str, limit: int = 60) -> str:
    text = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    return re.sub(r"-+", "-", text)[:limit].strip("-") or "section"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


# ---------------------------------------------------------------------------
# 1. Catalogue
# ---------------------------------------------------------------------------


def load_manifest() -> list[dict]:
    if not MANIFEST.exists():
        return []
    return tomllib.loads(MANIFEST.read_text())["file"]


def source_files() -> list[Path]:
    files = []
    for path in sorted(SOURCES.rglob("*")):
        rel = path.relative_to(SOURCES)
        if not path.is_file() or rel.parts[0] in GENERATED or rel.name.startswith("."):
            continue
        files.append(path)
    return files


def words_per_page(pdf: Path) -> float:
    """Average words per page in a PDF's text layer (0 for a scan)."""
    try:
        text = subprocess.run(["pdftotext", str(pdf), "-"], capture_output=True, text=True, check=True).stdout
        info = subprocess.run(["pdfinfo", str(pdf)], capture_output=True, text=True, check=True).stdout
        pages = int(re.search(r"^Pages:\s+(\d+)", info, re.M).group(1))
        return len(text.split()) / max(pages, 1)
    except (subprocess.CalledProcessError, AttributeError):
        return 0.0


def guess_entry(path: Path) -> dict:
    """A best guess for a new file. It is marked for review."""
    rel = path.relative_to(SOURCES).as_posix()
    lower = rel.lower()
    ext = path.suffix.lower()
    entry = {"path": rel, "id": slugify(path.stem), "title": path.stem, "origin": "", "review": "yes"}
    if "spec" in lower:
        entry["type"] = "spec"
    elif "markscheme" in lower or "mark scheme" in lower or lower.endswith(" ms.pdf"):
        entry["type"], entry["part"] = "exam-questions", "mark-scheme"
    elif "exam" in lower or "question" in lower or lower.endswith(" qp.pdf"):
        entry["type"], entry["part"] = "exam-questions", "questions"
    elif "obsidian" in lower or "student" in lower:
        entry["type"] = "student-notes"
    elif "pmt" in lower or ext in (".html", ".htm"):
        entry["type"] = "web-notes"
    elif "textbook" in lower or "heathcote" in lower:
        entry["type"] = "textbook"
    else:
        entry["type"] = "unknown"
    entry["trust"] = TRUST_BY_TYPE.get(entry["type"], "unknown")

    if ext == ".pdf":
        entry["method"] = "text" if words_per_page(path) >= 40 else "ocr"
    elif ext in (".md", ".markdown", ".txt"):
        entry["method"] = "markdown"
    elif ext in (".html", ".htm", ".docx"):
        entry["method"] = "text"
    elif ext in (".png", ".jpg", ".jpeg", ".webp"):
        entry["method"] = "image"
        entry["description"] = ""
    else:
        entry["method"] = "skip"
    number = re.search(r"\b(\d\.\d\.\d)\b", path.name)
    entry["spec"] = [number.group(1)] if number else []
    if entry["type"] == "unknown":
        entry["method"] = "skip"
    return entry


def toml_value(value) -> str:
    if isinstance(value, list):
        return "[" + ", ".join(json.dumps(v) for v in value) + "]"
    return json.dumps(value)


def catalogue() -> list[dict]:
    entries = load_manifest()
    known = {e["path"] for e in entries}
    new = [guess_entry(p) for p in source_files() if p.relative_to(SOURCES).as_posix() not in known]
    missing = [e["path"] for e in entries if not (SOURCES / e["path"]).exists()]
    if missing:
        say("These manifest entries point at files that no longer exist:")
        for path in missing:
            say(f"  {path}")
    if new:
        with MANIFEST.open("a") as f:
            for e in new:
                f.write("\n[[file]]\n" + "".join(f"{k} = {toml_value(v)}\n" for k, v in e.items()))
        say(f"Added {len(new)} new file(s) to sources/manifest.toml with a guessed type:")
        for e in new:
            say(f"  {e['type']:15} {e['method']:9} {e['path']}")
        say("Check them (and remove `review = \"yes\"`). Files of type \"unknown\" are skipped until you classify them.")
        entries = load_manifest()
    return [e for e in entries if (SOURCES / e["path"]).exists()]


# ---------------------------------------------------------------------------
# 2. Convert
# ---------------------------------------------------------------------------

_models = None


def marker_models():
    """Load Marker's models once (they take about a minute to load)."""
    global _models
    if _models is None:
        os.environ.setdefault("CUDA_DEVICE_ORDER", "PCI_BUS_ID")
        os.environ.setdefault("TORCH_DEVICE", "cuda")
        say("  loading Marker models...")
        from marker.models import create_model_dict

        _models = create_model_dict()
    return _models


def marker_convert(path: Path, force_ocr: bool) -> str:
    """Convert a PDF or image with Marker. Pages are marked <!-- page N --> (1-based)."""
    from marker.config.parser import ConfigParser
    from marker.converters.pdf import PdfConverter

    config = {
        "output_format": "markdown",
        "paginate_output": True,
        "disable_image_extraction": True,
        "force_ocr": force_ocr,
        "disable_ocr": not force_ocr,
    }
    parser = ConfigParser(config)
    converter = PdfConverter(
        config=parser.generate_config_dict(),
        artifact_dict=marker_models(),
        processor_list=parser.get_processors(),
        renderer=parser.get_renderer(),
    )
    markdown = converter(str(path)).markdown
    # Marker marks each page as "{0}-----..." (0-based).
    return re.sub(r"^\{(\d+)\}-+\s*$", lambda m: f"<!-- page {int(m.group(1)) + 1} -->", markdown, flags=re.M)


def layout_text(path: Path) -> str:
    """Plain text that keeps the page layout (pdftotext -layout). Used for exam
    papers and mark schemes: on these, Marker sometimes drops whole text blocks
    or scrambles table rows, while this keeps every word in order."""
    info = subprocess.run(["pdfinfo", str(path)], capture_output=True, text=True, check=True).stdout
    pages = int(re.search(r"^Pages:\s+(\d+)", info, re.M).group(1))
    out = []
    for number in range(1, pages + 1):
        text = subprocess.run(
            ["pdftotext", "-layout", "-f", str(number), "-l", str(number), str(path), "-"],
            capture_output=True, text=True, check=True,
        ).stdout
        out.append(f"<!-- page {number} -->\n" + "\n".join(l.rstrip() for l in text.split("\n")))
    return "\n".join(out)


def second_ocr(path: Path, entry: dict) -> None:
    """Read every page of a scan with Tesseract too. Marker sometimes drops text
    blocks (boxed quotes, sidebars, the book's index); comparing with this second
    reading lets the split stage put that text back. Cached per page."""
    from concurrent.futures import ThreadPoolExecutor

    folder = SECOND_OCR / entry["id"]
    folder.mkdir(parents=True, exist_ok=True)
    info = subprocess.run(["pdfinfo", str(path)], capture_output=True, text=True, check=True).stdout
    pages = int(re.search(r"^Pages:\s+(\d+)", info, re.M).group(1))

    def read(number: int) -> None:
        out = folder / f"p{number:03d}.txt"
        if out.exists():
            return
        image = folder / f"p{number:03d}"
        subprocess.run(["pdftoppm", "-f", str(number), "-l", str(number), "-r", "150", "-gray", "-png",
                        "-singlefile", str(path), str(image)], check=True)
        text = subprocess.run(["tesseract", f"{image}.png", "-", "--psm", "3"],
                              capture_output=True, text=True).stdout
        Path(f"{image}.png").unlink()
        out.write_text(text)

    say(f"  second OCR (Tesseract) of {pages} pages")
    with ThreadPoolExecutor(max_workers=8) as pool:
        list(pool.map(read, range(1, pages + 1)))


def html_to_markdown(html: str) -> str:
    from markdownify import markdownify

    return markdownify(html, heading_style="ATX")


def convert_markdown_note(path: Path, entries: list[dict]) -> str:
    """Student notes: embedded pictures are read with OCR, linked pictures
    replaced by their description from the manifest, LaTeX made readable."""
    text = path.read_text()
    images = {Path(e["path"]).name: e for e in entries if e.get("method") == "image"}
    IMAGES.mkdir(parents=True, exist_ok=True)

    def embedded(match: re.Match) -> str:
        data = base64.b64decode(match.group(2))
        image = IMAGES / f"{hashlib.sha1(data).hexdigest()[:12]}.{match.group(1)}"
        image.write_bytes(data)
        words = re.sub(r"<!-- page \d+ -->", "", marker_convert(image, force_ocr=True)).strip()
        if words:
            return "\n> Picture in the note (text read by OCR):\n> " + words.replace("\n", "\n> ") + "\n"
        return "\n> [Picture in the note with no text]\n"

    def linked(match: re.Match) -> str:
        name = match.group(1).split("|")[0].strip()
        entry = images.get(name)
        if entry and entry.get("description"):
            return f"\n> Picture in the note: {entry['description']}\n"
        return "\n> [Picture not included]\n"

    text = re.sub(r"!\[[^\]]*\]\(data:image/(\w+);base64,([A-Za-z0-9+/=\s]+)\)", embedded, text)
    text = re.sub(r"!\[\[([^\]]+)\]\]", linked, text)
    return "<!-- page 1 -->\n" + text


def transcribe_handwriting(path: Path, entry: dict) -> str:
    """Handwritten pages: render each page to an image and ask a Haiku agent
    (through the Claude Code CLI) to transcribe it. Marked as unverified."""
    folder = PAGES / entry["id"]
    folder.mkdir(parents=True, exist_ok=True)
    if path.suffix.lower() == ".pdf":
        subprocess.run(["pdftoppm", "-r", "150", "-png", str(path), str(folder / "p")], check=True)
        images = sorted(folder.glob("p-*.png"))
    else:
        images = [path]
    prompt = (
        "Read the image at {image} with the Read tool. It is a page of handwritten A level Computer "
        "Science revision notes. Transcribe it into Markdown exactly as written: keep headings, lists, "
        "tables, code and diagrams-as-text, keep the writer's own spelling and mistakes, and write "
        "[illegible] for anything you cannot read. Leave out any person's name, email address or "
        "other personal details. Reply with only the transcription."
    )
    pages = []
    for number, image in enumerate(images, start=1):
        say(f"  transcribing page {number} of {len(images)} with Haiku")
        # The prompt goes in on stdin: --allowedTools takes several values and
        # would swallow a prompt given after it.
        result = subprocess.run(
            ["claude", "-p", "--model", "haiku", "--allowedTools", "Read"],
            input=prompt.format(image=image.resolve()),
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        if result.returncode != 0:
            raise RuntimeError(f"transcription failed: {result.stderr.strip()[:300]}")
        pages.append(
            f"<!-- page {number} -->\n> **Unverified transcription** of handwriting, made by an AI model. "
            f"Check against the original before relying on it.\n\n{result.stdout.strip()}\n"
        )
    return "\n".join(pages)


def convert_file(entry: dict, entries: list[dict]) -> str:
    path = SOURCES / entry["path"]
    method = entry["method"]
    ext = path.suffix.lower()
    if method == "handwritten":
        return transcribe_handwriting(path, entry)
    if method == "markdown":
        return convert_markdown_note(path, entries)
    if method == "ocr":
        second_ocr(path, entry)
        return marker_convert(path, force_ocr=True)
    if method == "text":
        if ext in (".html", ".htm"):
            return "<!-- page 1 -->\n" + html_to_markdown(path.read_text(errors="replace"))
        if ext == ".docx":
            import mammoth

            with path.open("rb") as f:
                return "<!-- page 1 -->\n" + html_to_markdown(mammoth.convert_to_html(f).value)
        if entry.get("type") == "exam-questions":
            return layout_text(path)
        return marker_convert(path, force_ocr=False)
    raise ValueError(f"unknown method {method!r}")


def convert(entries: list[dict], only: str | None, method: str | None = None) -> None:
    RAW.mkdir(parents=True, exist_ok=True)
    cache = json.loads(CACHE_FILE.read_text()) if CACHE_FILE.exists() else {}
    # A note's conversion depends on the descriptions of its pictures too.
    image_text = "".join(e.get("description", "") for e in entries if e.get("method") == "image")
    todo = []
    for entry in entries:
        if entry.get("method") in ("skip", "image") or entry.get("type") == "unknown":
            continue
        if only and only not in entry["path"] and only != entry["id"]:
            continue
        if method and entry["method"] != method:
            continue
        key = sha256(SOURCES / entry["path"]) + entry["method"] + CONVERTER_VERSION
        if entry.get("type") == "exam-questions":
            key += "layout"
        if entry["method"] == "markdown":
            key += hashlib.sha1(image_text.encode()).hexdigest()
        if cache.get(entry["id"]) == key and (RAW / f"{entry['id']}.md").exists():
            continue
        todo.append((entry, key))

    if not todo:
        say("Convert: nothing new or changed.")
        return
    say(f"Convert: {len(todo)} file(s) to convert.")
    for number, (entry, key) in enumerate(todo, start=1):
        say(f"[{number}/{len(todo)}] {entry['method']:11} {entry['path']}")
        markdown = convert_file(entry, entries)
        (RAW / f"{entry['id']}.md").write_text(markdown)
        cache[entry["id"]] = key
        CACHE_FILE.write_text(json.dumps(cache, indent=1))  # saved after each file, so a stop loses little


# ---------------------------------------------------------------------------
# 2b. Diagrams (optional: tools/convert-sources --diagrams)
#
# OCR drops diagrams. This pass finds them on scanned pages, crops each one and
# asks a local vision model (Gemma 4 12B, run with llama.cpp on the RTX 4070)
# to transcribe it: tables as Markdown tables, circuits/trees/graphs as
# Mermaid. Results are saved per diagram, so a stopped run carries on where it
# left off. The split stage then adds them to their page, clearly labelled.
# ---------------------------------------------------------------------------

LLAMA_DIR = ROOT / "tools" / "llama.cpp"
LLAMA_SERVER = LLAMA_DIR / "llama-b11239" / "llama-server"
LLAMA_LIBS = [LLAMA_DIR / "llama-b11239", LLAMA_DIR / "cudart-llama-b11239-bin-ubuntu-cuda-13.4-x64"]
DIAGRAM_MODEL = LLAMA_DIR / "models" / "gemma-4-12B-it-Q4_0.gguf"
DIAGRAM_MMPROJ = LLAMA_DIR / "models" / "mmproj-gemma-4-12B-it-Q8_0.gguf"
DIAGRAM_MODEL_NAME = "Gemma 4 12B"
DIAGRAM_PORT = 8081

DIAGRAM_PROMPT = """This image is one diagram cropped from a page of an A level Computer Science textbook.
Transcribe it so that someone who cannot see it gets exactly the same information.

First line: `Type: ` followed by one of: karnaugh-map, truth-table, table, logic-circuit, tree, graph,
flowchart, state-diagram, er-diagram, data-structure, network, chart, screenshot, photo, other.

Then, depending on the type:
- karnaugh-map, truth-table, table: a Markdown table with every row label, column label and cell exactly
  as shown. Leave empty cells empty. Do not describe loops, outlines or groupings drawn over the cells:
  the surrounding text explains those.
- logic-circuit: a Mermaid `flowchart LR` code block with one node per input, gate and output. Label each
  gate node with its type and a number (AND1, OR1, NOT1, XOR1, NAND1, NOR1). Draw one edge for every wire,
  exactly as connected.
- tree, graph, flowchart, state-diagram, er-diagram, data-structure, network: a Mermaid code block
  (flowchart TD, stateDiagram-v2 or erDiagram) with every node, edge, label and weight exactly as shown.
  For a binary tree keep each left child before its right child. Put labels in quotes, like A["x > 5"].
- chart, screenshot, photo, other: two or three plain sentences saying what it shows, including any
  readable text.
If there is a caption or label printed with the diagram, add it last as `Caption: ...`.

Rules: copy labels, numbers and symbols exactly as printed (Boolean symbols are ∧ ∨ ¬ ⊻ ≡). Write [unclear]
for anything you cannot read. Do not explain, interpret, simplify or add anything that is not in the image.
Reply with only the transcription."""


def parse_pages(text: str | None) -> set[int] | None:
    """ "238-257,300" -> {238, ..., 257, 300} (PDF page numbers)."""
    if not text:
        return None
    pages = set()
    for part in text.split(","):
        a, _, b = part.partition("-")
        pages.update(range(int(a), int(b or a) + 1))
    return pages


def find_diagrams(entry: dict, wanted: set[int] | None) -> None:
    """Find Picture/Figure boxes with the layout model and crop them. Cached in layout.json."""
    import pypdfium2
    folder = DIAGRAMS / entry["id"]
    folder.mkdir(parents=True, exist_ok=True)
    done_file = folder / "layout.json"
    done = json.loads(done_file.read_text()) if done_file.exists() else {}
    pdf = pypdfium2.PdfDocument(str(SOURCES / entry["path"]))
    todo = [n for n in range(1, len(pdf) + 1) if str(n) not in done and (wanted is None or n in wanted)]
    if not todo:
        return
    say(f"  finding diagrams on {len(todo)} page(s)")
    from surya.foundation import FoundationPredictor
    from surya.layout import LayoutPredictor
    from surya.settings import settings as surya_settings

    predictor = LayoutPredictor(FoundationPredictor(checkpoint=surya_settings.LAYOUT_MODEL_CHECKPOINT))
    for start in range(0, len(todo), 8):
        batch = todo[start:start + 8]
        low = [pdf[n - 1].render(scale=96 / 72).to_pil().convert("RGB") for n in batch]
        results = predictor(low)
        for n, image, result in zip(batch, low, results):
            boxes = []
            page_area = image.width * image.height
            for box in result.bboxes:
                x0, y0, x1, y1 = box.bbox
                if (x1 - x0) * (y1 - y0) < page_area * 0.01:
                    continue
                # Real equations in this book are a line or two tall; a tall
                # "Equation" box is a diagram the model was unsure about
                # (Karnaugh maps often come out that way).
                tall_equation = box.label == "Equation" and (y1 - y0) >= 60
                if box.label in ("Picture", "Figure") or tall_equation:
                    boxes.append([x0, y0, x1, y1])
            boxes.sort(key=lambda b: (b[1], b[0]))  # top to bottom
            if boxes:
                high = pdf[n - 1].render(scale=200 / 72).to_pil().convert("RGB")
                k = 200 / 96
                for i, (x0, y0, x1, y1) in enumerate(boxes, start=1):
                    crop = high.crop((max(0, x0 * k - 12), max(0, y0 * k - 12),
                                      min(high.width, x1 * k + 12), min(high.height, y1 * k + 12)))
                    crop.save(folder / f"p{n:03d}-{i}.png")
            done[str(n)] = boxes
            done_file.write_text(json.dumps(done))
    del predictor
    try:
        import torch
        torch.cuda.empty_cache()
    except Exception:
        pass


def start_diagram_model():
    """Start llama-server with the vision model, unless one is already running. Returns the process or None."""
    import urllib.request
    try:
        urllib.request.urlopen(f"http://127.0.0.1:{DIAGRAM_PORT}/health", timeout=2)
        return None
    except Exception:
        pass
    if not DIAGRAM_MODEL.exists():
        raise RuntimeError(f"{DIAGRAM_MODEL} is missing; see tools/llama.cpp")
    env = dict(os.environ)
    env["LD_LIBRARY_PATH"] = ":".join(str(p) for p in LLAMA_LIBS)
    log = open(WORK / "diagram-model.log", "ab")
    process = subprocess.Popen(
        [str(LLAMA_SERVER), "-m", str(DIAGRAM_MODEL), "--mmproj", str(DIAGRAM_MMPROJ), "-ngl", "99",
         # A whole image must fit in one batch (-b/-ub), or llama.cpp aborts.
         "-c", "16384", "-b", "2048", "-ub", "2048", "-np", "1", "-fa", "on", "--cache-type-k", "q8_0", "--cache-type-v", "q8_0",
         "--host", "127.0.0.1", "--port", str(DIAGRAM_PORT), "--jinja"],
        env=env, stdout=log, stderr=subprocess.STDOUT,
    )
    import time
    for _ in range(120):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{DIAGRAM_PORT}/health", timeout=2)
            return process
        except Exception:
            if process.poll() is not None:
                raise RuntimeError("the diagram model failed to start; see sources/.work/diagram-model.log")
            time.sleep(2)
    process.terminate()
    raise RuntimeError("the diagram model did not start within 4 minutes")


def describe_image(image: Path) -> str:
    import io
    import urllib.request
    from PIL import Image

    # Big crops become too many image tokens: shrink so the long side is at most 1600 px.
    picture = Image.open(image)
    picture.thumbnail((1600, 1600))
    buffer = io.BytesIO()
    picture.save(buffer, format="PNG")
    data = base64.b64encode(buffer.getvalue()).decode()
    body = json.dumps({
        "messages": [{"role": "user", "content": [
            {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{data}"}},
            {"type": "text", "text": DIAGRAM_PROMPT},
        ]}],
        "temperature": 0,
        "max_tokens": 1200,
        # Gemma 4 "thinks" first by default and can use up every token before
        # answering. Transcribing does not need it.
        "chat_template_kwargs": {"enable_thinking": False},
    }).encode()
    request = urllib.request.Request(
        f"http://127.0.0.1:{DIAGRAM_PORT}/v1/chat/completions", data=body,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=300) as response:
        reply = json.loads(response.read())
    text = (reply["choices"][0]["message"].get("content") or "").strip()
    if not text:
        raise RuntimeError("empty answer")
    return text


def describe_diagrams(entry: dict, wanted: set[int] | None) -> None:
    folder = DIAGRAMS / entry["id"]
    images = sorted(folder.glob("p*.png"))
    if wanted is not None:
        images = [i for i in images if int(i.name[1:4]) in wanted]
    todo = [i for i in images if not i.with_suffix(".md").exists()]
    if not todo:
        say("  all diagrams already described")
        return
    say(f"  describing {len(todo)} diagram(s) with {DIAGRAM_MODEL_NAME}")
    process = start_diagram_model()
    failed = []
    try:
        for number, image in enumerate(todo, start=1):
            for attempt in range(3):
                try:
                    text = describe_image(image)
                    image.with_suffix(".md").write_text(text + "\n")
                    break
                except Exception as error:  # timeouts, server hiccups
                    # If the server has died, start it again and carry on.
                    if process is not None and process.poll() is not None:
                        say(f"  the diagram model stopped (while on {image.name}); restarting it")
                        process = start_diagram_model()
                    if attempt == 2:
                        failed.append(image.name)
                        say(f"  could not describe {image.name}: {error}")
            if number % 10 == 0:
                say(f"  {number}/{len(todo)} described")
    finally:
        if process:
            process.terminate()
            process.wait(timeout=30)
    if failed:
        say(f"  {len(failed)} diagram(s) failed; re-run with --diagrams to try them again")


def diagram_block(entry_id: str, page: int) -> str:
    """The descriptions of the diagrams on one page, labelled as AI-made."""
    blocks = []
    for path in sorted((DIAGRAMS / entry_id).glob(f"p{page:03d}-*.md")):
        text = path.read_text().strip()
        # Mermaid styling (colours, comments) says nothing about the content.
        text = "\n".join(
            line for line in text.split("\n")
            if not re.match(r"\s*(style|classDef|class|linkStyle)\s|\s*%%", line)
        )
        blocks.append(
            f"**[AI description of a diagram on this page, made by {DIAGRAM_MODEL_NAME}: "
            f"check the original before relying on it]**\n\n{text}"
        )
    return "\n\n".join(blocks)


# ---------------------------------------------------------------------------
# 3. Split: clean each document and cut it into sections
# ---------------------------------------------------------------------------

URL = re.compile(
    r"[\w.+-]+@[\w-]*(?:\.[\w-]+)*|https?://\S+|\bwww\.[^\s)|\]]+|\b[\w-]+\.(?:com|co\.uk|org\.uk|org|education|net|ac\.uk|gov\.uk)\b(?:/\S*)?",
    re.I,
)
LATEX = {
    r"\wedge": "∧", r"\land": "∧", r"\vee": "∨", r"\lor": "∨", r"\neg": "¬", r"\lnot": "¬",
    r"\veebar": "⊻", r"\oplus": "⊕", r"\equiv": "≡", r"\leftrightarrow": "↔", r"\rightarrow": "→",
    r"\leftarrow": "←", r"\times": "×", r"\div": "÷", r"\le": "≤", r"\leq": "≤", r"\ge": "≥",
    r"\geq": "≥", r"\ne": "≠", r"\neq": "≠", r"\cdot": "·", r"\ldots": "...", r"\dots": "...",
}
MARKS = re.compile(r"\[(\d{1,2})\]")
COMMAND_WORDS = [
    "Analyse", "Annotate", "Calculate", "Compare", "Complete", "Convert", "Construct", "Create",
    "Define", "Describe", "Design", "Determine", "Discuss", "Draw", "Evaluate", "Explain", "Give",
    "Identify", "Justify", "Label", "List", "Name", "Outline", "Refine", "Rewrite", "Show", "Simplify",
    "Sketch", "State", "Suggest", "Trace", "Use", "What", "Which", "Why", "How", "Write",
]
STOPWORDS = set(
    "the and for with from that this into their there which what when where how why are was were use "
    "used using its can will each other than then them they have has had not but all any one two may "
    "such also more most between different including include see appendix learners should".split()
)


def latex_to_text(text: str) -> str:
    r"""Turn simple LaTeX maths ($A \wedge B$) into plain text (A ∧ B)."""

    def replace(match: re.Match) -> str:
        maths = match.group(1) or match.group(2) or ""
        for command, symbol in sorted(LATEX.items(), key=lambda kv: -len(kv[0])):
            maths = re.sub(re.escape(command) + r"(?![a-zA-Z])", symbol, maths)
        maths = re.sub(r"\\(?:text|mathrm|textbf|mathbf|operatorname)\{([^}]*)\}", r"\1", maths)
        maths = re.sub(r"\\(?:begin|end)\{[^}]*\}", "", maths)
        maths = maths.replace(r"\\", "\n").replace(r"\ ", " ").replace("&", "")
        maths = re.sub(r"\\overline\{([^}]*)\}", r"¬(\1)", maths)
        maths = re.sub(r"[{}]", "", maths)
        return re.sub(r"[ \t]+", " ", maths).strip()

    return re.sub(r"\$\$(.+?)\$\$|\$(.+?)\$", replace, text, flags=re.S)


_known_words: set[str] | None = None


def known_words(book_text: str) -> set[str]:
    """English words plus every word in the main OCR text: used to tell real text from OCR noise."""
    global _known_words
    if _known_words is None:
        words = set()
        for wordlist in WORDLISTS:
            if wordlist.exists():
                words.update(w.strip().lower() for w in wordlist.read_text(errors="ignore").split())
        _known_words = words
    return _known_words | set(re.findall(r"[a-z]+", book_text.lower()))


def recovered_lines(entry_id: str, number: int, page_text: str, vocabulary: set[str]) -> list[str]:
    """Lines from the second OCR that the main OCR missed and that are made of real words."""
    path = SECOND_OCR / entry_id / f"p{number:03d}.txt"
    if not path.exists():
        return []
    have = set(re.findall(r"[a-z]{4,}", page_text.lower()))
    lines = []
    for line in path.read_text().split("\n"):
        line = line.strip()
        tokens = re.findall(r"[A-Za-z]+|\d+", line)
        words = [w.lower() for w in re.findall(r"[A-Za-z]+", line)]
        long_words = [w for w in words if len(w) >= 4]
        index_entry = re.fullmatch(r"[A-Za-z][\w\s'()/-]*,\s*\d+(\s*[,-]\s*\d+)*", line)
        if (len(tokens) < 3 and not index_entry) or not long_words or re.match(r"(CHAPTER|SECTION)\s+\d+", line):
            continue
        # Web addresses and phone numbers (e.g. in screenshots of search results),
        # which OCR often breaks up so the URL filter would not catch them.
        if re.search(r"(?i)\b(www|https?|htips|htitp)\b|\bco\.? ?u[kl]\b|\.com\b|@|\d{4,5} ?\d{6}", line):
            continue
        real = sum(1 for w in words if w in vocabulary) / len(words)
        missing = sum(1 for w in long_words if w not in have) / len(long_words)
        if real >= 0.8 and missing >= 0.6:
            lines.append(line)
    return lines


def load_redactions() -> list[str]:
    if not REDACT.exists():
        REDACT.write_text(
            "# Personal details to remove from the converted text and the index.\n"
            "# Add one [[remove]] entry per name, nickname, email address and so on.\n"
            "# Matching ignores upper/lower case. This file stays in sources/, which is never committed.\n"
        )
    data = tomllib.loads(REDACT.read_text())
    return [item["text"] for item in data.get("remove", []) if item.get("text")]


def redact(text: str, redactions: list[str], student_notes: bool) -> str:
    for item in redactions:
        text = re.sub(re.escape(item), "[removed]", text, flags=re.I)
    if student_notes:
        # Obsidian tags (#something) can be nicknames or in-jokes: drop them all.
        text = re.sub(r"(?<![\w#])#[A-Za-z][\w/-]*", "", text)
    return text


def flatten_sparse_tables(text: str) -> str:
    """Answer-space boxes come out as tables of mostly empty cells: turn them into plain lines."""
    out, block = [], []

    def flush() -> None:
        rows = [r for r in block if not re.fullmatch(r"\|[\s|:-]*\|?", r.strip())]
        cells = [c.strip() for r in rows for c in r.strip().strip("|").split("|")]
        filled = [c for c in cells if c]
        if cells and len(filled) / len(cells) < 0.4:
            for r in rows:
                line = " ".join(c.strip() for c in r.strip().strip("|").split("|") if c.strip())
                # Keep a question number recognisable ("1." would read as a list item).
                line = re.sub(r"^(\d{1,2}(?:\([a-z]\))?\.)(?=\s)", r"**\1**", line)
                if line:
                    out.append(line)
        else:
            out.extend(block)
        block.clear()

    for line in text.split("\n"):
        if line.lstrip().startswith("|"):
            block.append(line)
        else:
            if block:
                flush()
            out.append(line)
    if block:
        flush()
    return "\n".join(out)


def split_pages(raw: str) -> list[tuple[int, str]]:
    parts = re.split(r"<!-- page (\d+) -->", raw)
    pages = []
    if parts[0].strip():
        pages.append((1, parts[0]))
    for number, body in zip(parts[1::2], parts[2::2]):
        pages.append((int(number), body))
    return pages


def edge_lines(body: str, count: int = 3) -> list[str]:
    """The first and last few non-empty lines of a page: where headers and footers sit."""
    lines = [l for l in body.split("\n") if l.strip()]
    return lines[:count] + lines[-count:]


def furniture_lines(pages: list[tuple[int, str]]) -> set[str]:
    """Lines repeated at the top or bottom of many pages (running headers and footers)."""
    if len(pages) < 3:
        return set()
    counts: dict[str, int] = {}
    for _, body in pages:
        for line in {normalise(l) for l in edge_lines(body) if not MARKS.search(l)}:
            if line and len(line) < 100:
                counts[line] = counts.get(line, 0) + 1
    # Only lines with real words count: "2   i   3" in a mark scheme also
    # repeats once digits are ignored, but it is content, not a header.
    return {
        line for line, n in counts.items()
        if n >= max(3, len(pages) * 0.4) and len(re.sub(r"[^a-z]", "", line)) >= 6
    }


def normalise(line: str) -> str:
    return re.sub(r"[\d#*_|\s]+", " ", line).strip().lower()


def clean_page(body: str, furniture: set[str], textbook: bool) -> tuple[str, int | None]:
    """Remove page furniture from one page. Returns the text and its printed page number."""
    printed = None
    kept = []
    lines = body.split("\n")
    edges = set(edge_lines(body))
    for index, line in enumerate(lines):
        bare = line.strip().strip("*#_ ").strip()
        if line in edges and normalise(line) in furniture and not line.lstrip().startswith("|") and not MARKS.search(line):
            continue
        if textbook:
            if re.fullmatch(r"\d{1,3}", bare) and (index < 3 or index >= len(lines) - 4):
                printed = int(bare)
                continue
            if re.fullmatch(r"\d{1,2}\s*[-–]\s*\d{1,2}", bare):  # section tabs like "8-40"
                continue
            if re.match(r"(?:CHAPTER|SECTION)\s+\d+\s*[-—–~]", bare):  # running headers
                continue
        kept.append(line)
    text = "\n".join(kept)
    text = URL.sub("", text)
    return re.sub(r"\n{3,}", "\n\n", text).strip(), printed


def page_label(first: int, last: int, printed: dict[int, int] | None = None) -> str:
    pdf = f"{first}" if first == last else f"{first}-{last}"
    if printed:
        a, b = printed.get(first), printed.get(last)
        if a and b:
            book = f"{a}" if a == b else f"{a}-{b}"
            return f"book {book} (PDF {pdf})"
    return f"PDF {pdf}" if first else "1"


def infer_printed_pages(found: dict[int, int], all_pages: list[int]) -> dict[int, int]:
    """Fill in printed page numbers from the ones OCR found, using the nearest known offset."""
    offsets = {pdf: pdf - printed for pdf, printed in found.items() if 0 < pdf - printed < 40}
    if not offsets:
        return {}
    result = {}
    known = sorted(offsets)
    for pdf in all_pages:
        nearest = min(known, key=lambda k: abs(k - pdf))
        result[pdf] = pdf - offsets[nearest]
    return {k: v for k, v in result.items() if v > 0}


def front_matter(fields: dict) -> str:
    lines = ["---"]
    for key, value in fields.items():
        lines.append(f"{key}: {json.dumps(value, ensure_ascii=False)}")
    return "\n".join(lines) + "\n---\n\n"


def write_section(folder: Path, number: int, slug: str, fields: dict, body: str) -> Path:
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / f"{number:02d}-{slugify(slug)}.md"
    path.write_text(front_matter(fields) + body.strip() + "\n")
    return path


def base_fields(entry: dict, title: str, pages: str, spec: list[str]) -> dict:
    return {
        "title": title,
        "source": entry["title"],
        "type": entry["type"],
        "trust": entry["trust"],
        "pages": pages,
        "spec": spec,
    }


def cleaned_pages(entry: dict, redactions: list[str], flatten: bool = True) -> tuple[list[tuple[int, str]], dict[int, int]]:
    raw = (RAW / f"{entry['id']}.md").read_text()
    pages = split_pages(raw)
    scanned = entry.get("method") == "ocr"
    if scanned:
        pages = drop_repeated_pages(pages, entry["id"])
        vocabulary = known_words(raw)
    furniture = furniture_lines(pages)
    textbook = entry["type"] == "textbook"
    result, printed = [], {}
    for number, body in pages:
        text, book_page = clean_page(body, furniture, textbook)
        if scanned:
            missed = recovered_lines(entry["id"], number, body, vocabulary)
            if missed:
                text += (
                    "\n\n**[Text on this page that the main OCR missed, recovered by a second OCR "
                    "(Tesseract): check the original]**\n\n" + URL.sub("", "\n".join(missed))
                )
        if entry.get("method") == "ocr":
            diagrams = diagram_block(entry["id"], number)
            if diagrams:
                text += "\n\n" + URL.sub("", diagrams)
        text = latex_to_text(text)
        if flatten:
            text = flatten_sparse_tables(text)
        text = redact(text, redactions, entry["type"] == "student-notes")
        result.append((number, text))
        if book_page:
            printed[number] = book_page
    if textbook:
        printed = infer_printed_pages(printed, [n for n, _ in pages])
    return result, printed


def drop_repeated_pages(pages: list[tuple[int, str]], entry_id: str) -> list[tuple[int, str]]:
    """A scan can contain the same page twice. Drop a page whose words are 90% the same as the page before."""
    kept: list[tuple[int, str]] = []
    for number, body in pages:
        if kept:
            a = set(re.findall(r"[a-z]{4,}", kept[-1][1].lower()))
            b = set(re.findall(r"[a-z]{4,}", body.lower()))
            if len(a) > 30 and len(b) > 30 and len(a & b) / len(a | b) >= 0.9:
                say(f"  note: {entry_id} PDF page {number} repeats page {kept[-1][0]}; skipped")
                continue
        kept.append((number, body))
    return kept


def join_pages(pages: list[tuple[int, str]]) -> str:
    return "\n\n".join(f"<!-- page {n} -->\n{text}" for n, text in pages if text.strip())


def split_by_page_ranges(entry, pages, sections, folder) -> list[Path]:
    written = []
    for number, section in enumerate(sections, start=1):
        first, last = section["pages"]
        chosen = [(n, t) for n, t in pages if first <= n <= last]
        fields = base_fields(entry, section["title"], page_label(first, last), section["spec"])
        written.append(write_section(folder, number, section["slug"], fields, join_pages(chosen)))
    return written


def split_textbook(entry, pages, printed, chapters, folder, index_book_page=None) -> list[Path]:
    """One file per chapter, found by its "Chapter N" title (the contents pages come first)."""
    numbers = [n for n, _ in pages]
    text_of = dict(pages)
    starts = {}
    search_from = 10
    titles = {}
    for chapter, title, *_ in chapters:
        pattern = re.compile(rf"(?m)^\W*Chapter\s*{chapter}\b(?!\s*\S*\s*\d+\s*$)")
        for n in numbers:
            if n >= search_from and pattern.search(text_of[n]):
                # A "Section N ... In this section" page just before belongs with this chapter.
                # A section's intro page lists its chapters ("In this section: Chapter 40 ..."):
                # the chapter's own title page is then the next page.
                if "in this section" in text_of[n].lower():
                    titles[chapter] = n + 1
                else:
                    titles[chapter] = n
                    if n - 1 in text_of and "in this section" in text_of[n - 1].lower():
                        n -= 1
                starts[chapter] = n
                search_from = n + 1
                break
    index_start = next(
        (n for n in numbers if n > search_from and re.search(r"(?m)^#*\s*\**Index\**\s*$", text_of[n])), None
    )
    # Book page numbers: each chapter's first page from the contents page, counted on from its title page.
    book_pages = {}
    position = {n: i for i, n in enumerate(numbers)}  # skips repeated scan pages
    for chapter, title, spec, *rest in chapters:
        if rest and chapter in titles:
            for n in numbers:
                if n >= starts[chapter]:
                    book_pages[n] = rest[0] + (position[n] - position[titles[chapter]])
    if index_start and index_book_page:
        for n in numbers:
            if n >= index_start:
                book_pages[n] = index_book_page + (position[n] - position[index_start])
    printed = book_pages or printed
    boundaries = [(0, 1, "Front matter and contents", [])]
    for chapter, title, spec, *_ in chapters:
        if chapter in starts:
            boundaries.append((chapter, starts[chapter], f"Chapter {chapter}: {title}", spec))
        else:
            say(f"  warning: could not find the start of chapter {chapter} ({title})")
    if index_start:
        boundaries.append((len(chapters) + 1, index_start, "Index", []))
    ranges = []
    for i, (number, first, title, spec) in enumerate(boundaries):
        last = boundaries[i + 1][1] - 1 if i + 1 < len(boundaries) else numbers[-1]
        ranges.append((number, first, last, title, spec))
    # If one chapter's book pages don't run straight on from the last, the scan
    # probably has a page missing or scanned twice there: say "about".
    unsure = set()
    for (a, _, a_last, a_title, _), (b, b_first, _, b_title, _) in zip(ranges, ranges[1:]):
        if a and b and a_last in printed and b_first in printed and printed[b_first] != printed[a_last] + 1:
            unsure.update({a, b})
    if unsure:
        say(f"  note: book page numbers are approximate for chapters {sorted(unsure)}: "
            "the scan may have a page missing or repeated there")
    written = []
    for number, first, last, title, spec in ranges:
        chosen = [(n, t) for n, t in pages if first <= n <= last]
        label = page_label(first, last, printed)
        if number in unsure:
            label = label.replace("book ", "book about ")
        fields = base_fields(entry, title, label, spec)
        slug = title.split(": ", 1)[-1]
        written.append(write_section(folder, number, slug, fields, join_pages(chosen)))
    return written


def split_by_headings(entry, pages, folder, min_words: int = 80) -> list[Path]:
    """Cut at level 1-2 headings; sections shorter than min_words join the one before."""
    sections: list[dict] = []
    for number, text in pages:
        for line in text.split("\n"):
            heading = re.match(r"^#{1,2}\s+(.+)$", line)
            if heading or not sections:
                title = re.sub(r"[*_#]", "", heading.group(1)).strip() if heading else entry["title"]
                title = re.sub(r"^[●•\-\s]+|^\d+\.\s+", "", title).strip() or entry["title"]
                if sections and len(sections[-1]["text"].split()) < min_words:
                    sections[-1]["text"] += "\n" + line
                    sections[-1]["last"] = number
                    continue
                sections.append({"title": title, "first": number, "last": number, "text": ""})
            sections[-1]["text"] += "\n" + line
            sections[-1]["last"] = number
    written = []
    for number, section in enumerate(sections, start=1):
        fields = base_fields(entry, section["title"], page_label(section["first"], section["last"]), entry["spec"])
        written.append(write_section(folder, number, section["title"], fields, section["text"]))
    return written


# A question starts with "**5.**", "1(a)." (bold or not) or a number at the
# start of a table cell ("| 4. |"). Plain "1. " is a list item, not a question.
QUESTION_START = re.compile(
    r"^[\s#]*(?:\*\*(\d{1,2})(?:\([a-z]\))?\.\*\*|\**(\d{1,2})\([a-z]\)\.|\|[\s|]*\**(\d{1,2})(?:\([a-z]\))?\.\**(?:\s|\||$))"
    r"|^\s{0,2}(\d{1,2})(?:\([a-z]\))?\.(?:\s+\S|\s*$)"
)


def question_number(line: str) -> int | None:
    match = QUESTION_START.match(line)
    return int(next(g for g in match.groups() if g)) if match else None


def split_questions(text: str) -> list[tuple[int, str]]:
    """Cut a question paper at each new question number. Marker sometimes puts
    questions slightly out of order, so any number not seen yet counts, as long
    as it is at most two ahead of the highest so far."""
    questions: list[tuple[int, list[str]]] = []
    seen: set[int] = set()
    page = "1"
    for line in text.split("\n"):
        marker = re.match(r"<!-- page (\d+) -->", line)
        if marker:
            page = marker.group(1)
        number = question_number(line)
        if number and number not in seen and number <= max(seen, default=0) + 2:
            seen.add(number)
            # Start with the page marker, so the question knows where it is.
            questions.append((number, [f"<!-- page {page} -->"]))
        if questions and not (marker and questions[-1][1] == [line]):
            questions[-1][1].append(line)
    return sorted((n, "\n".join(lines)) for n, lines in questions)


MS_LINE = re.compile(r"^(\s{0,15})(\d{1,2})(?=\s|$)(.*)$")


def split_mark_scheme(text: str) -> dict[int, str]:
    """Group a mark scheme (layout text) by question. A question number sits at
    the left margin, followed by a part letter, a roman numeral, a wide gap or
    nothing. Narrow columns split "12" over two lines ("1", then "2" two lines
    below in the same column)."""
    lines = [l for l in text.split("\n") if not l.startswith("<!-- page")]
    groups: dict[int, list[str]] = {}
    question = None
    skip = set()

    def is_start(rest: str) -> bool:
        return bool(re.match(r"\s*$|\s+[a-h]\b|\s+[ivx]{1,4}\b|\s{3,}", rest))

    for i, line in enumerate(lines):
        if i in skip:
            groups.setdefault(question, []).append(line) if question else None
            continue
        match = MS_LINE.match(line)
        number = None
        if match and is_start(match.group(3)):
            column, digits = len(match.group(1)), match.group(2)
            limit = max(groups, default=0) + 2
            if len(digits) == 1:
                for j in range(i + 1, min(i + 4, len(lines))):
                    second = MS_LINE.match(lines[j])
                    if second and len(second.group(1)) == column and len(second.group(2)) == 1:
                        two = int(digits + second.group(2))
                        if two not in groups and two <= limit:
                            number = two
                            skip.add(j)
                        break
            if number is None and int(digits) not in groups and 1 <= int(digits) <= limit:
                number = int(digits)
        if number:
            # The question number sits in the middle of its table row, so lines
            # just above it already belong to it. Move everything after the
            # previous question's "Total N" line across to the new question.
            if question and groups[question]:
                previous = groups[question]
                totals = [k for k, l in enumerate(previous) if re.match(r"\s*Total\b", l)]
                if totals and totals[-1] < len(previous) - 1:
                    moved = previous[totals[-1] + 1:]
                    del previous[totals[-1] + 1:]
                else:
                    moved = []
            else:
                moved = []
            question = number
            groups[question] = moved
        if question:
            groups[question].append(line)
    result = {}
    for q, body in groups.items():
        block = "\n".join(body).strip("\n")
        indent = min((len(l) - len(l.lstrip()) for l in body if l.strip()), default=0)
        block = "\n".join(l[indent:] for l in block.split("\n"))
        result[q] = re.sub(r"\n{3,}", "\n\n", block)
    return result


def command_words(question: str) -> list[str]:
    found = []
    for line in question.split("\n"):
        line = re.sub(r"^[\W\d]*(?:\(?[a-z]{1,4}\)?[.)]\s*)?", "", line.strip())
        word = re.match(r"[A-Z][a-z]+", line)
        if word and word.group(0) in COMMAND_WORDS and word.group(0) not in found:
            found.append(word.group(0))
    return found


def split_exam(entry, pages, mark_scheme_entry, redactions, folder) -> list[Path]:
    text = join_pages(pages)
    scheme = {}
    if mark_scheme_entry and (RAW / f"{mark_scheme_entry['id']}.md").exists():
        ms_pages, _ = cleaned_pages(mark_scheme_entry, redactions, flatten=False)
        scheme = split_mark_scheme(join_pages(ms_pages))
    written = []
    component = "01 Computer systems" if entry["spec"] and entry["spec"][0].startswith("1") else "02 Algorithms and programming"
    text = re.sub(r" {8,}", "      ", text)
    for number, body in split_questions(text):
        page_numbers = [int(n) for n in re.findall(r"<!-- page (\d+) -->", body)] or [0]
        marks = sum(int(m) for m in MARKS.findall(body))
        if not marks and number in scheme:
            totals = re.findall(r"^\s*Total\s+(\d{1,2})\s*$", scheme[number], re.M)
            marks = int(totals[-1]) if totals else 0
        fields = base_fields(entry, f"{entry['title']}, question {number}", page_label(min(page_numbers), max(page_numbers)), entry["spec"])
        fields.update({
            "paper": f"H446 component {component} (paper not stated in source)",
            "year": "not stated in source",
            "question": number,
            "marks": marks if marks else "not found",
            "command_words": command_words(body),
            "mark_scheme": "included" if number in scheme else "not available",
        })
        pages_in = re.findall(r"<!-- page \d+ -->", body)
        question_text = "\n".join(l.rstrip() for l in re.sub(r"<!-- page \d+ -->\n?", "", body).split("\n"))
        out = "## Question\n\n```text\n" + question_text.strip("\n") + "\n```"
        if number in scheme:
            out += "\n\n## Mark scheme\n\n```text\n" + re.sub(r" {8,}", "      ", scheme[number]).strip("\n") + "\n```"
        written.append(write_section(folder, number, f"question-{number}", fields, out))
    return written


def split_all(entries: list[dict]) -> list[Path]:
    if TEXT.exists():
        shutil.rmtree(TEXT)
    maps = tomllib.loads(MAPS.read_text()) if MAPS.exists() else {}
    redactions = load_redactions()
    by_path = {e["path"]: e for e in entries}
    written: list[Path] = []
    note_number = 0
    for entry in entries:
        if not (RAW / f"{entry['id']}.md").exists() or entry.get("part") == "mark-scheme":
            continue
        folder = TEXT / entry["type"] / entry["id"]
        pages, printed = cleaned_pages(entry, redactions)
        spec_sections = [s for s in maps.get("spec_sections", []) if s["source"] == entry["id"]]
        chapter_map = next((c for c in maps.get("chapters", []) if c["source"] == entry["id"]), None)
        chapters = chapter_map["list"] if chapter_map else None
        if spec_sections:
            written += split_by_page_ranges(entry, pages, spec_sections, folder)
        elif chapters:
            written += split_textbook(entry, pages, printed, chapters, folder, chapter_map.get("index_book_page"))
        elif entry["type"] == "exam-questions":
            written += split_exam(entry, pages, by_path.get(entry.get("mark_scheme", "")), redactions, folder)
        elif entry["type"] == "student-notes":
            note_number += 1
            folder = TEXT / "student-notes" / "obsidian-notes" if "Obsidian" in entry["path"] else folder
            fields = base_fields(entry, entry["title"], "1", entry["spec"])
            written.append(write_section(folder, note_number, entry["title"].split(": ", 1)[-1], fields, join_pages(pages)))
        else:
            written += split_by_headings(entry, pages, folder)
    say(f"Split: {len(written)} section files in sources/text/")
    return written


# ---------------------------------------------------------------------------
# 4. Index
# ---------------------------------------------------------------------------


def spec_points() -> dict[str, dict]:
    data = tomllib.loads(SPEC_TOML.read_text())
    return {p["number"]: p for p in data["point"]}


def keywords(text: str) -> set[str]:
    words = re.findall(r"[a-z][a-z0-9-]{3,}", text.lower())
    return {w.rstrip("s") for w in words if w not in STOPWORDS}


def infer_sub_points(point: dict, signals: str) -> list[str]:
    """Lettered sub-points whose distinctive words appear in the headings/question stems."""
    subs = point.get("sub", [])
    if not subs:
        return []
    found = keywords(signals)
    counts: dict[str, int] = {}
    sub_words = []
    for sub in subs:
        words = keywords(sub["text"] + " " + " ".join(sub.get("items", [])))
        sub_words.append(words)
        for w in words:
            counts[w] = counts.get(w, 0) + 1
    letters = []
    for sub, words in zip(subs, sub_words):
        distinctive = {w for w in words if counts[w] == 1 and len(w) >= 5}
        if distinctive & found:
            letters.append(sub["letter"])
    return letters


def signals_of(path: Path) -> str:
    """Headings, bold lines and question stems only: not the whole text."""
    lines = path.read_text().split("\n")
    picked = [l for l in lines if l.startswith("#") or re.match(r"^\*\*[^*]+\*\*\s*$", l.strip())]
    if "/exam-questions/" in path.as_posix():
        picked += [l for l in lines if QUESTION_START.match(l) or re.match(r"^\W*[ivx]{1,4}\.\s", l)]
    return "\n".join(picked)


def read_front_matter(path: Path) -> dict:
    text = path.read_text()
    block = text.split("---\n", 2)[1]
    fields = {}
    for line in block.strip().split("\n"):
        key, value = line.split(": ", 1)
        fields[key] = json.loads(value)
    return fields


def build_index(files: list[Path]) -> None:
    points = spec_points()
    lines = [
        "# Source index",
        "",
        "One line per section file in sources/text/: title | type | trust | pages | spec points.",
        "Spec points come from the source (file names, the textbook contents page, hand-checked maps).",
        "Lettered sub-points in brackets are inferred from headings and question stems, so treat them as a guide.",
        "Built by tools/convert-sources; do not edit by hand.",
        "",
    ]
    for path in sorted(files):
        fields = read_front_matter(path)
        signals = signals_of(path)
        tags = []
        for number in fields["spec"]:
            if number == "all" or number not in points:
                continue
            letters = [] if fields["type"] == "spec" else infer_sub_points(points[number], signals)
            tags.append(f"{number}({','.join(letters)})" if letters else number)
        rel = path.relative_to(SOURCES).as_posix()
        extra = f" | {fields['marks']} marks" if isinstance(fields.get("marks"), int) else ""
        lines.append(
            f"- [{fields['title']}]({rel}) | {fields['type']} | {fields['trust']} | {fields['pages']} | "
            f"{' '.join(tags) or '-'}{extra}"
        )
    INDEX.write_text("\n".join(lines) + "\n")
    say(f"Index: {len(files)} lines in sources/index.md")


# ---------------------------------------------------------------------------
# 5. Check
# ---------------------------------------------------------------------------


def check(files: list[Path]) -> bool:
    redactions = load_redactions()
    problems = []
    for path in files + [INDEX]:
        text = path.read_text()
        if URL.search(text):
            problems.append(f"{path.relative_to(SOURCES)}: contains a URL ({URL.search(text).group(0)})")
        for item in redactions:
            if item.lower() in text.lower():
                problems.append(f"{path.relative_to(SOURCES)}: contains an item from redact.toml")
    for problem in problems:
        say("  " + problem)
    say("Check: OK, no URLs or listed personal details left." if not problems else f"Check: {len(problems)} problem(s).")
    return not problems


def main() -> None:
    args = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    args.add_argument("--only", help="convert only files whose path contains this text (or this id)")
    args.add_argument("--method", help="convert only files with this method (text, ocr, markdown, handwritten)")
    args.add_argument("--no-convert", action="store_true", help="skip stage 2; just re-split and re-index")
    args.add_argument("--diagrams", action="store_true",
                      help="also find and describe diagrams in scanned sources (slow; resumable)")
    args.add_argument("--pages", help="with --diagrams: only these PDF pages, e.g. 238-257,300")
    options = args.parse_args()

    entries = catalogue()
    if not options.no_convert:
        convert(entries, options.only, options.method)
    if options.diagrams:
        wanted = parse_pages(options.pages)
        for entry in entries:
            if entry.get("method") != "ocr" or (options.only and options.only not in entry["path"] and options.only != entry["id"]):
                continue
            say(f"Diagrams: {entry['path']}")
            find_diagrams(entry, wanted)
            describe_diagrams(entry, wanted)
    files = split_all(entries)
    build_index(files)
    if not check(files):
        sys.exit(1)


if __name__ == "__main__":
    main()
