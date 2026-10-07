"""Redraw the diagrams in exam-questions/ as text, mostly with a local AI model.

Run it with `tools/redraw-exam-diagrams`. tools/extract-exam-questions copies
the text of the OCR papers but leaves a note where each diagram was. This:

1. finds each of those diagrams in the question paper or mark scheme PDF and
   crops it (into sources/.work/exam-diagrams/, never committed);
2. asks Gemma 4 12B, run locally with llama.cpp on the RTX 4070, to redraw it
   from an enlarged crop: Mermaid for trees, graphs and diagrams, a Markdown
   table for tables and Karnaugh maps, a code block for code, a quote for a
   candidate's handwritten answer, a sentence for anything else. Tiny images
   in mark schemes are the examiner's marking symbols and just get a note;
3. redraws logic circuits with Claude Sonnet via `claude -p` (the user's plan),
   twice, comparing truth tables, because the local model gets crossing wires
   wrong. With --review, Claude also checks the local model's tables, trees,
   graphs and code and corrects any mistakes (about 8-10k tokens an image);
4. checks every Mermaid block with the real Mermaid parser in a headless
   browser, asks the model to fix any that fail, and shows the rest as plain
   text rather than a broken drawing;
5. puts each redrawing in place of its note, labelled as made by an AI model
   (naming it).

Every step is cached, so a re-run only does new work. Notes that have already
been replaced are left alone, so hand corrections to a redrawing are safe.
"""

from __future__ import annotations

import argparse
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import convert  # noqa: E402  (the local vision model helpers)
import exam_text as ex  # noqa: E402  (reads the papers and writes the files)

ROOT = Path(__file__).resolve().parents[2]
EXAM_DIR = ROOT / "exam-questions"
WORK = ROOT / "sources" / ".work" / "exam-diagrams"
MERMAID_JS = ROOT / "static" / "vendor" / "mermaid-12.0.0.min.js"
CHECKER = Path(__file__).with_name("check_mermaid.js")

MODEL = convert.DIAGRAM_MODEL_NAME
CIRCUIT_MODEL = "Claude Sonnet"
MODELS_FILE = WORK / "models.json"  # which model made each redrawing (default: MODEL)
NOTE = {"question": ex.DIAGRAM_NOTE, "answer": ex.IMAGE_NOTE, "guidance": ex.IMAGE_NOTE}
# Tiny images in mark schemes are the examiner's marking symbols (a tick, a
# question mark in a circle), which mean nothing to a student.
MARK_SYMBOL = "*[An examiner's marking symbol, not shown here.]*"
SYMBOL_SIZE = 130  # pixels at 200 dpi, about 1.7cm
# Every redrawing starts with one of these labels; the pattern finds them on re-runs.
REDRAWN_START = {
    "question": "*[Diagram redrawn from the original paper by an AI model",
    "answer": "*[Image redrawn from the original mark scheme by an AI model",
}
REDRAWN_START["guidance"] = REDRAWN_START["answer"]


def redrawn_label(kind: str, model: str) -> str:
    return f"{REDRAWN_START[kind]} ({model}). It may contain mistakes.]*"


def key(job: dict) -> str:
    return job["png"].relative_to(WORK).as_posix()

PROMPT = """This image is one diagram cropped from an OCR A Level Computer Science exam paper or its mark scheme.
Redraw it as text so that a student who cannot see it has exactly the same information.

First line: `Type: ` followed by one of: karnaugh-map, truth-table, trace-table, table, logic-circuit, tree,
graph, flowchart, state-diagram, er-diagram, class-diagram, data-structure, network, code, handwritten, other.

Then, depending on the type:
- karnaugh-map, truth-table, trace-table, table: a Markdown table with every row label, column label and cell
  exactly as shown. Leave empty cells empty; write ■ for a shaded (filled in) cell. Numbers written outside a
  grid (such as nonogram clues) go in an extra first or last row or column; every row has the same number of
  cells. Do not describe loops or groupings drawn over the cells.
- logic-circuit: a ```mermaid code block starting with `flowchart LR`. One node per input, gate and output.
  Name gates AND1, OR1, NOT1, XOR1, NAND1, NOR1 and label them with the gate type, like AND1["AND"].
  Draw one edge for every wire, exactly as connected.
- tree, graph, flowchart, state-diagram, er-diagram, class-diagram, data-structure, network: a ```mermaid
  code block (flowchart TD or LR, stateDiagram-v2, erDiagram or classDiagram) with every node, edge, label and
  weight exactly as shown. Write a weighted edge as A ---|5| B. In a binary tree label every edge with the
  side of its child, like K ---|left| G and K ---|right| L, so a lone child's side is not lost.
  A table of values with a pointer (such as an array holding a stack or queue) is a Markdown table, with the
  pointers written as sentences underneath.
- code (program code, pseudocode, assembly language or Little Man Computer, SQL, HTML, CSS, or a sum set out
  in columns such as a binary addition): a ```text code block with every line and every digit exactly as
  shown, keeping indentation, alignment and blank lines. Do not summarise it.
- handwritten (a candidate's handwritten answer): the line `A candidate's handwritten answer:` then the words
  exactly as written, keeping printed headings and line breaks, as a Markdown quote (each line starting `> `).
  Write [unclear] for any word you cannot read with confidence; never guess.
- other (a screenshot, photo, chart, or an empty box or grid for the answer): one or two plain sentences
  saying what it shows, including any readable text.

Mermaid rules: node ids are plain letters and digits (A, N1, AND1); put every label in double quotes, like
N1["x > 5"]; no style, class or comment lines.
Copy labels, numbers and symbols exactly (Boolean symbols are ∧ ∨ ¬ ⊻ ≡). Write [unclear] for anything you
cannot read. Do not answer the question, explain, or add anything that is not in the image.
Reply with only the redrawing."""

FIX = """That Mermaid code does not parse: {error}
Reply with the whole redrawing again, with the Mermaid code fixed. Node ids must be plain letters and digits,
every label in double quotes, and no style lines. Do not change what the diagram shows."""


def say(message: str) -> None:
    print(message, flush=True)


# ---------------------------------------------------------------------------
# 1. Find and crop the diagrams
# ---------------------------------------------------------------------------


def slug(label: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")


def jobs_for(spec: str, qp: Path, ms: Path) -> list[dict]:
    """Every diagram behind a note, with where it goes in the exam file."""
    jobs = []
    for question in ex.read_questions(qp):
        for part in question.parts:
            for k, (page, box) in enumerate(part.images):
                jobs.append({"kind": "question", "question": question.number, "label": part.label, "k": k,
                             "pdf": qp, "page": page, "box": box,
                             "name": f"q{question.number}-{slug(part.label)}-{k + 1}"})
    rows, _ = ex.read_mark_scheme(ms)
    row_number: dict[int, int] = {}
    for row in rows:
        q = int(re.match(r"\d+", row.label).group(0))
        index = row_number.get(q, 0)
        row_number[q] = index + 1
        for kind, images in (("answer", row.answer_images), ("guidance", row.guidance_images)):
            for k, (page, box) in enumerate(images):
                jobs.append({"kind": kind, "question": q, "row": index, "k": k, "pdf": ms, "page": page, "box": box,
                             "name": f"ms-q{q}-row{index + 1}-{kind}-{k + 1}"})
    for job in jobs:
        job["png"] = WORK / spec / f"{job['name']}.png"
    return jobs


def crop(job: dict) -> None:
    if job["png"].exists():
        return
    import pdfplumber

    job["png"].parent.mkdir(parents=True, exist_ok=True)
    with pdfplumber.open(job["pdf"]) as pdf:
        page = pdf.pages[job["page"] - 1]
        x0, top, x1, bottom = job["box"]
        box = (max(0, x0 - 4), max(0, top - 4), min(page.width, x1 + 4), min(page.height, bottom + 4))
        page.crop(box).to_image(resolution=200).original.save(job["png"])


# ---------------------------------------------------------------------------
# 2. Redraw with the local model
# ---------------------------------------------------------------------------


def tidy_mermaid(text: str) -> str:
    """Fix slips the models make often: empty Mermaid labels ("") do not
    parse, and a handwritten answer's blank lines come out as runs of empty
    quote lines."""
    text = re.sub(r'\[""\]', '[" "]', text)
    return re.sub(r"(\n>[ \t]*)(\n>[ \t]*)+(?=\n)", "\n>", text)


def runaway(text: str) -> bool:
    """True if the model got stuck repeating itself (e.g. "[unclear]" hundreds of times)."""
    # Only repeats with letters in count, so rows of empty table cells ("| | |") do not.
    return any(re.search("[A-Za-z]", m.group(1)) for m in re.finditer(r"((?:\S+\s+){1,4}?)\1{10,}", text))


CIRCUIT_PROMPT = """This image is a logic circuit cropped from an OCR A Level Computer Science exam paper or its
mark scheme. Redraw it as text so that a student who cannot see it has exactly the same information.

Some wires cross. A wire only joins another where there is a dot or a T-junction; where two wires cross
without a dot they are NOT connected, so follow each one straight on. An XOR gate is an OR shape with a second
curved line just behind its inputs: check every OR-shaped gate for that extra line. A small circle on an
output means NOT (NAND, NOR, or a NOT gate's triangle).

Step 1. Under the heading `Wiring:`, go through every gate from left to right and, for each of its inputs,
trace the wire back to where it starts (an input letter, or another gate's output), saying which way it runs
and what it crosses. Then check each input letter: list every gate its wire reaches.

Step 2. Write the line `Type: logic-circuit`, then a ```mermaid code block starting with `flowchart LR`.
One node per input, gate and output. Name gates AND1, OR1, NOT1, XOR1, NAND1, NOR1 and label them with the
gate type, like AND1["AND"]. Draw one edge for every wire, exactly as your tracing found.
(If the image is not a logic circuit after all, skip step 1 and follow the instructions below.)

""" + PROMPT.split("\n\n", 1)[1]


def ask_claude(prompt: str) -> str:
    """Ask Claude Sonnet through the Claude Code CLI (the user's plan, not API
    billing). A short system prompt and only the Read tool (to see the image)
    keep each call to a few thousand tokens."""
    result = subprocess.run(
        ["claude", "-p", "--model", "sonnet", "--tools", "Read", "--allowedTools", "Read",
         "--system-prompt", "You redraw diagrams from exam papers as text, exactly as instructed.",
         "--strict-mcp-config", "--setting-sources", ""],
        input=prompt, capture_output=True, text=True, timeout=600, cwd=ROOT,
    )
    if result.returncode != 0 or not result.stdout.strip():
        raise RuntimeError(result.stderr.strip()[:300] or "empty answer")
    return result.stdout.strip()


def claude_redraw(image: Path) -> str:
    """Redraw a logic circuit with Claude. Local models get the wiring wrong
    where wires cross, and exam questions depend on it."""
    answer = ask_claude(f"Read the image at {image.resolve()} with the Read tool.\n\n{CIRCUIT_PROMPT}")
    if "Type:" not in answer:
        raise RuntimeError(f"no redrawing in: {answer[:200]}")
    # Keep only the redrawing: the wiring notes are there to make the model look carefully.
    return answer[answer.find("Type:"):].strip()


REVIEW_PROMPT = """Below is a text redrawing of that image, made by another AI model for students who cannot see
it. Compare it with the image, detail by detail: every node, edge, label, cell, digit and which side each child
is on. If it is exactly right, reply with only the word OK. Otherwise reply with the whole corrected redrawing
(starting with its `Type:` line), following the instructions it was made with:

<instructions>
{prompt}
</instructions>

<redrawing>
{redrawing}
</redrawing>"""

# Types the local model often gets wrong in detail, so --review has Claude check them.
REVIEW_TYPES = {"tree", "graph", "table", "truth-table", "trace-table", "karnaugh-map", "data-structure",
                "flowchart", "state-diagram", "er-diagram", "class-diagram", "network", "code", "other"}


def review(jobs: list[dict], models: dict) -> None:
    """Have Claude check the local model's redrawings of the types above and
    correct any that are wrong. A redrawing that is right costs only "OK"."""
    todo = []
    for job in jobs:
        md = job["png"].with_suffix(".md")
        if md.exists() and key(job) not in models:
            kind = md.read_text().split("\n", 1)[0].removeprefix("Type: ").strip()
            if kind in REVIEW_TYPES:
                todo.append(job)
    if not todo:
        return
    say(f"  checking {len(todo)} redrawing(s) with {CIRCUIT_MODEL}")
    for job in todo:
        md = job["png"].with_suffix(".md")
        image = enlarged(job["png"])
        prompt = (f"Read the image at {image.resolve()} with the Read tool.\n\n"
                  + REVIEW_PROMPT.format(prompt=PROMPT, redrawing=md.read_text().strip()))
        try:
            answer = ask_claude(prompt)
        except Exception as error:
            say(f"  {job['name']}: {CIRCUIT_MODEL} failed ({error})")
            continue
        if answer.strip().strip(".") == "OK":
            models[key(job)] = f"{MODEL}, checked by {CIRCUIT_MODEL}"
        elif "Type:" in answer:
            job["old"] = redrawing(job, models)  # so update_file can swap it for the correction
            md.write_text(tidy_mermaid(answer[answer.find("Type:"):].strip()) + "\n")
            models[key(job)] = f"{MODEL}, corrected by {CIRCUIT_MODEL}"
            say(f"  {job['name']}: corrected")
        else:
            say(f"  {job['name']}: unexpected answer: {answer[:120]}")
            continue
        MODELS_FILE.write_text(json.dumps(models, indent=1))


def enlarged(png: Path) -> Path:
    """A copy of the crop at least 1200 pixels across. Claude reads the gate
    shapes (OR against XOR) far more reliably from a bigger picture."""
    from PIL import Image

    big = png.with_suffix(".big.png")
    if not big.exists():
        image = Image.open(png)
        scale = max(1, min(4, -(-1200 // max(image.size))))  # -(-a // b) rounds up
        image.resize((image.width * scale, image.height * scale), Image.LANCZOS).save(big)
    return big


GATES = {
    "AND": all, "OR": any, "NOT": lambda v: not v[0], "XOR": lambda v: sum(v) % 2 == 1,
    "NAND": lambda v: not all(v), "NOR": lambda v: not any(v), "XNOR": lambda v: sum(v) % 2 == 0,
}
NODE = re.compile(r'^(\w+)(?:\["([^"]*)"\])?$')
ARROW = re.compile(r"\s*--[->](?:\|[^|]*\|)?\s*")


def truth_table(text: str) -> tuple | None:
    """Work out what a Mermaid circuit computes: its output for every input
    combination. Two redrawings with the same truth table describe the same
    circuit, even if they number their gates differently. None if the drawing
    is not a circuit this can follow."""
    labels, inputs_of = {}, {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith(("```", "flowchart", "Type:")):
            continue
        names = []
        for token in ARROW.split(line):
            match = NODE.match(token.strip())
            if not match:
                return None
            name, label = match.groups()
            labels[name] = label if label is not None else labels.get(name, name)
            names.append(name)
        for source, target in zip(names, names[1:]):
            inputs_of.setdefault(target, []).append(source)
    # Inputs are matched by letter: a mark scheme may draw two circuits, each with its own A.
    inputs = sorted({labels[n] for n in labels if n not in inputs_of})
    outputs = sorted((n for n in labels if not any(n in s for s in inputs_of.values())), key=lambda n: labels[n])
    if not inputs or len(inputs) > 6:
        return None

    def value(name, given, seen=()):
        if name not in inputs_of:
            return given[labels[name]]
        if name in seen:
            raise ValueError("loop")
        args = [value(n, given, seen + (name,)) for n in inputs_of[name]]
        gate = re.sub(r"\d+$", "", labels[name].strip().upper())
        if gate in GATES:
            return bool(GATES[gate](args))
        if len(args) == 1:  # an output letter or a labelled wire
            return args[0]
        raise ValueError(f"unknown gate {labels[name]}")

    rows = []
    for number in range(2 ** len(inputs)):
        given = {n: bool(number >> i & 1) for i, n in enumerate(reversed(inputs))}
        try:
            rows.append(tuple(value(n, given) for n in outputs))
        except (ValueError, KeyError, IndexError):
            return None
    return tuple(labels[n] for n in inputs), tuple(labels[n] for n in outputs), tuple(rows)


def redraw_circuit(job: dict) -> tuple[str, bool]:
    """Redraw a circuit twice with Claude; if the two disagree, a third time and
    take the majority. Returns the redrawing and whether it was agreed."""
    big = enlarged(job["png"])
    answers = []
    for _ in range(3):
        answers.append(tidy_mermaid(claude_redraw(big)))
        tables = [truth_table(a) for a in answers]
        for i, table in enumerate(tables):
            if table is not None and tables.count(table) >= 2:
                return answers[i], True
        if len(answers) == 2 and tables[0] is None and answers[0] == answers[1]:
            return answers[0], True
    return answers[0], False


def redraw_circuits(jobs: list[dict], models: dict) -> None:
    circuits = [j for j in jobs if j["png"].with_suffix(".md").exists()
                and j["png"].with_suffix(".md").read_text().startswith("Type: logic-circuit")
                and models.get(key(j)) not in (CIRCUIT_MODEL, "unagreed")]
    if not circuits:
        return
    say(f"  redrawing {len(circuits)} logic circuit(s) with {CIRCUIT_MODEL}")
    for job in circuits:
        try:
            text, agreed = redraw_circuit(job)
        except Exception as error:
            say(f"  {job['name']}: {CIRCUIT_MODEL} failed ({error}); keeping the {MODEL} redrawing")
            continue
        if not agreed:
            say(f"  {job['name']}: three redrawings disagree; check it by hand against {job['png']}")
        job["png"].with_suffix(".md").write_text(text + "\n")
        models[key(job)] = CIRCUIT_MODEL if agreed else "unagreed"
        MODELS_FILE.write_text(json.dumps(models, indent=1))


def redraw(jobs: list[dict]) -> None:
    for job in jobs:
        if job["kind"] != "question" and not job["png"].with_suffix(".md").exists():
            from PIL import Image

            if max(Image.open(job["png"]).size) < SYMBOL_SIZE:
                job["png"].with_suffix(".md").write_text("Type: mark-symbol\n")
    todo = [j for j in jobs if not j["png"].with_suffix(".md").exists()]
    if not todo:
        return
    say(f"  redrawing {len(todo)} diagram(s) with {MODEL}")
    process = convert.start_diagram_model()
    try:
        for n, job in enumerate(todo, start=1):
            for attempt in range(3):
                try:
                    # The model reads small print and handwriting far better enlarged.
                    text = tidy_mermaid(convert.describe_image(enlarged(job["png"]), PROMPT))
                    if runaway(text):
                        raise RuntimeError("the answer repeats itself")
                    job["png"].with_suffix(".md").write_text(text + "\n")
                    break
                except Exception as error:
                    if process is not None and process.poll() is not None:
                        process = convert.start_diagram_model()
                    if attempt == 2:
                        say(f"  could not redraw {job['name']}: {error}")
            if n % 20 == 0:
                say(f"  {n}/{len(todo)}")
    finally:
        if process:
            process.terminate()
            process.wait(timeout=30)


# ---------------------------------------------------------------------------
# 3. Check the Mermaid code
# ---------------------------------------------------------------------------

MERMAID_BLOCK = re.compile(r"```mermaid\n(.*?)```", re.S)


def checker_paths() -> tuple[str, str] | None:
    """A node_modules folder with playwright, and a Chromium to run it with."""
    modules = sorted(glob.glob(os.path.expanduser("~/.npm/_npx/*/node_modules/playwright")))
    chromes = sorted(glob.glob(os.path.expanduser("~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome")))
    if not modules or not chromes or not MERMAID_JS.exists():
        return None
    return str(Path(modules[-1]).parent), chromes[-1]


def parse_errors(codes: list[str], paths: tuple[str, str]) -> list[str | None]:
    if not codes:
        return []
    result = subprocess.run(
        ["node", str(CHECKER), str(MERMAID_JS), paths[1]], input=json.dumps(codes),
        capture_output=True, text=True, env={**os.environ, "NODE_PATH": paths[0]}, timeout=600,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip()[:300])
    return json.loads(result.stdout)


def check(jobs: list[dict]) -> None:
    """Parse every Mermaid block; ask the model to fix failures; show what still fails as plain text."""
    paths = checker_paths()
    if not paths:
        say("  note: no Chromium/playwright found to check Mermaid code; skipped (the site shows broken ones as text)")
        return
    checked_file = WORK / "checked.json"
    checked = json.loads(checked_file.read_text()) if checked_file.exists() else {}
    pending = []
    for job in jobs:
        md = job["png"].with_suffix(".md")
        if md.exists():
            text = md.read_text()
            if MERMAID_BLOCK.search(text) and checked.get(key(job)) != hashlib.sha1(text.encode()).hexdigest():
                pending.append((job, text))
    if not pending:
        return
    say(f"  checking {len(pending)} Mermaid redrawing(s)")
    process = None
    try:
        for round_ in range(3):
            # Test the text as it will go on the page, after cleaned().
            errors = parse_errors([MERMAID_BLOCK.search(cleaned(t)).group(1) for _, t in pending], paths)
            failing = []
            for (job, text), error in zip(pending, errors):
                if error is None:
                    checked[key(job)] = hashlib.sha1(text.encode()).hexdigest()
                else:
                    failing.append((job, text, error))
            if not failing:
                break
            if round_ == 2:
                for job, text, _ in failing:
                    text = text.replace("```mermaid\n", "```text\n", 1) + "\n(This diagram could not be drawn, so it is shown as code.)\n"
                    job["png"].with_suffix(".md").write_text(text)
                    checked[key(job)] = hashlib.sha1(text.encode()).hexdigest()
                say(f"  {len(failing)} redrawing(s) still did not parse; shown as code")
                break
            say(f"  {len(failing)} did not parse; asking the model to fix them")
            process = process or convert.start_diagram_model()
            pending = []
            for job, text, error in failing:
                history = [{"role": "assistant", "content": text},
                           {"role": "user", "content": FIX.format(error=error)}]
                try:
                    fixed = convert.describe_image(job["png"], PROMPT, history)
                except Exception:
                    fixed = text
                if not MERMAID_BLOCK.search(fixed):
                    fixed = text
                fixed = tidy_mermaid(fixed)
                job["png"].with_suffix(".md").write_text(fixed + ("\n" if not fixed.endswith("\n") else ""))
                pending.append((job, job["png"].with_suffix(".md").read_text()))
    finally:
        if process:
            process.terminate()
            process.wait(timeout=30)
        checked_file.write_text(json.dumps(checked, indent=1))


# ---------------------------------------------------------------------------
# 4. Put the redrawings into the exam files
# ---------------------------------------------------------------------------


def cleaned(answer: str) -> str:
    """A model's answer as it goes on the page: without its Type line and any
    colour or style lines. `class A,B name;` styles nodes in a flowchart, but
    `class Building {` in a class diagram is content, so it stays."""
    lines = [l for l in answer.strip().split("\n") if not l.startswith("Type:")
             and not re.match(r"\s*(style|classDef|linkStyle)\s|\s*%%|\s*class\s+[\w,]+\s+\w+\s*;?\s*$", l)]
    return convert.latex_to_text("\n".join(lines).strip())


def redrawing(job: dict, models: dict) -> str | None:
    md = job["png"].with_suffix(".md")
    if not md.exists():
        return None
    if md.read_text().startswith("Type: mark-symbol"):
        return MARK_SYMBOL
    text = cleaned(md.read_text())
    model = models.get(key(job), MODEL)
    label = redrawn_label(job["kind"], CIRCUIT_MODEL if model == "unagreed" else model)
    return f"{label}\n\n{text}" if text else None


def fill(text: str, kind: str, jobs: list[dict], where: str, models: dict) -> tuple[str, int]:
    """Replace the k-th note in `text` with the k-th job's redrawing. Notes
    already replaced still count, so positions stay lined up."""
    marker = re.compile("|".join([re.escape(NOTE[kind]), re.escape(MARK_SYMBOL), re.escape(REDRAWN_START[kind]) + r"[^\n]*"]))
    found = list(marker.finditer(text))
    if len(found) != len(jobs):
        if jobs:
            say(f"  {where}: {len(found)} note(s) but {len(jobs)} image(s); left alone")
        return text, 0
    done = 0
    for match, job in reversed(list(zip(found, jobs))):
        if match.group(0) == NOTE[kind]:
            end = match.end()
        elif job.get("old") and text.startswith(job["old"], match.start()):
            end = match.start() + len(job["old"])  # corrected by --review, and not edited by hand
        else:
            continue
        new = redrawing(job, models)
        if new:
            text = text[:match.start()] + new + text[end:]
            done += 1
    return text, done


def write_file(path: Path, data: dict, header: list[str]) -> None:
    """The same layout tools/extract-exam-questions writes."""
    lines = header + [f'spec = "{data["spec"]}"', ""]
    for q in data["question"]:
        lines += ["[[question]]", f"number = {q['number']}", f"marks = {q['marks']}"]
        if q.get("diagram"):
            lines.append("diagram = true")
        lines.append("")
        for part in q.get("part", []):
            lines += ["[[question.part]]", f'label = "{part["label"]}"']
            if "marks" in part:
                lines.append(f"marks = {part['marks']}")
            lines += ["text = " + ex.toml_string(part["text"]), ""]
        for row in q.get("mark_scheme", []):
            lines += ["[[question.mark_scheme]]", f'label = "{row["label"]}"']
            if "marks" in row:
                lines.append(f"marks = {row['marks']}")
            lines.append("answer = " + ex.toml_string(row["answer"]))
            if "guidance" in row:
                lines.append("guidance = " + ex.toml_string(row["guidance"]))
            lines.append("")
    path.write_text("\n".join(lines).rstrip() + "\n")


def update_file(spec: str, jobs: list[dict], models: dict) -> int:
    path = EXAM_DIR / f"{spec}.toml"
    original = path.read_text()
    data = tomllib.loads(original)
    header = [l for l in original.split("\n") if l.startswith("#")]
    done = 0
    for q in data["question"]:
        for part in q.get("part", []):
            mine = [j for j in jobs if j["kind"] == "question" and j["question"] == q["number"] and j["label"] == part["label"]]
            part["text"], n = fill(part["text"], "question", sorted(mine, key=lambda j: j["k"]), f"{spec} Q{part['label']}", models)
            done += n
        for index, row in enumerate(q.get("mark_scheme", [])):
            for kind in ("answer", "guidance"):
                if kind not in row:
                    continue
                mine = [j for j in jobs if j["kind"] == kind and j["question"] == q["number"] and j["row"] == index]
                row[kind], n = fill(row[kind], kind, sorted(mine, key=lambda j: j["k"]), f"{spec} Q{q['number']} mark scheme", models)
                done += n
        still_missing = any(ex.DIAGRAM_NOTE in part["text"] for part in q.get("part", []))
        q["diagram"] = still_missing
    if done:
        write_file(path, data, header)
    return done


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--only", help="only spec points starting with this, e.g. 1.4.3")
    parser.add_argument("--circuits", choices=["claude", "local"], default="claude",
                        help="who redraws logic circuits: Claude Sonnet via Claude Code (default, more accurate) or the local model")
    parser.add_argument("--review", action="store_true",
                        help="have Claude Sonnet check (and correct) the local model's tables, trees, graphs and code")
    args = parser.parse_args()
    models = json.loads(MODELS_FILE.read_text()) if MODELS_FILE.exists() else {}
    manifest = tomllib.loads(ex.MANIFEST.read_text())["file"]
    by_path = {e["path"]: e for e in manifest}
    papers = [e for e in manifest if e.get("type") == "exam-questions" and e.get("part") == "questions"]
    total = 0
    for entry in papers:
        spec = entry["spec"][0]
        if (args.only and not spec.startswith(args.only)) or not (EXAM_DIR / f"{spec}.toml").exists():
            continue
        ms = by_path[entry["mark_scheme"]]
        jobs = jobs_for(spec, ex.SOURCES / entry["path"], ex.SOURCES / ms["path"])
        if not jobs:
            continue
        say(f"{spec}: {len(jobs)} diagram(s)")
        for job in jobs:
            crop(job)
        redraw(jobs)
        if args.circuits == "claude":
            redraw_circuits(jobs, models)
        if args.review:
            review(jobs, models)
        check(jobs)
        done = update_file(spec, jobs, models)
        total += done
        if done:
            say(f"  {done} redrawing(s) added to exam-questions/{spec}.toml")
    say(f"Done: {total} redrawing(s) added.")


if __name__ == "__main__":
    main()
