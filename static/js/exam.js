// Exam questions: answers and marks, saved in this browser.
//
// Each part of a question is marked on its own:
// - Most parts: write your answer, open the mark scheme and tick each mark
//   point your answer makes. Your mark is worked out from your ticks (no more
//   than each mark scheme row's marks, and no more than the part's marks).
//   You can change the number yourself.
// - Parts with one right answer (listed in exam-questions/answers/): type the
//   answer and press "Check answer". "Change my mark" lets you overrule it,
//   for example when you showed working that earns marks.
//
// Saved in localStorage (see Store in site.js):
//   "h446:exam:<spec point>"
//     { "<question>": { "mark": 5, "of": 8, "parts": {
//         "3(a)": { "mark": 2, "of": 3, "ticks": ["m0-1"], "auto": "right", "changed": true } } } }
//   "h446:exam-answers:<spec point>"
//     { "3(a)": "a written answer", "3(b)": ["box 1", "box 2"] }
// The exam questions list shows each spec point's total from "mark" and "of".

(function () {
  "use strict";

  if (!window.Store) return;

  function loadJSON(key) {
    var data = null;
    try {
      data = JSON.parse(window.Store.read(key));
    } catch (e) {
      data = null;
    }
    return data && typeof data === "object" ? data : {};
  }

  function saveJSON(key, data) {
    window.Store.write(key, Object.keys(data).length ? JSON.stringify(data) : null);
  }

  // Adds up the saved marks: how many questions, marks scored, marks possible.
  function totals(marks) {
    var result = { questions: 0, mark: 0, of: 0 };
    Object.keys(marks).forEach(function (number) {
      var entry = marks[number];
      if (!entry || typeof entry.mark !== "number" || typeof entry.of !== "number") return;
      result.questions++;
      result.mark += entry.mark;
      result.of += entry.of;
    });
    return result;
  }

  function plural(count, word) {
    return count + " " + word + (count === 1 ? "" : "s");
  }

  // The exam questions list: one line per spec point.
  document.querySelectorAll("[data-exam-progress]").forEach(function (row) {
    var text = row.querySelector("[data-exam-text]");
    var sum = totals(loadJSON("exam:" + row.getAttribute("data-exam-progress")));
    if (text && sum.questions > 0) {
      text.textContent = "You: " + sum.mark + "/" + sum.of + " on " + plural(sum.questions, "question");
    }
  });

  // ---------------------------------------------------------------------------
  // Boolean expressions: reading them and comparing truth tables
  // ---------------------------------------------------------------------------

  // Turns what a student typed into a tree, e.g. "¬A ∧ B" into
  // { op: "∧", a: { op: "¬", a: { op: "var", name: "A" } }, b: { op: "var", name: "B" } }.
  // Throws an Error with a message for the student if it can't be read.
  // AND, OR, NOT, XOR, . + ! ~ & | ⊕ and a lowercase v for OR are accepted too.
  // Order (strongest first): ¬, ∧, ⊻, ∨, ≡. Brackets make anything clear.
  function parseExpression(text) {
    var source = text
      .replace(/\bXOR\b/gi, " ⊻ ")
      .replace(/\bAND\b/gi, " ∧ ")
      .replace(/\bOR\b/gi, " ∨ ")
      .replace(/\bNOT\b/gi, " ¬ ")
      .replace(/⊕/g, "⊻")
      .replace(/[&·.*]/g, "∧")
      .replace(/[+|]/g, "∨")
      .replace(/[~!]/g, "¬");

    var tokens = [];
    for (var i = 0; i < source.length; i++) {
      var c = source[i];
      if (/\s/.test(c)) continue;
      if (c === "v") tokens.push("∨");
      else if (/[A-Za-z]/.test(c)) tokens.push(c.toUpperCase());
      else if ("01∧∨¬⊻≡()".indexOf(c) >= 0) tokens.push(c);
      else if (c === "^") throw new Error("Use ∧ for AND, or ⊻ for XOR, instead of ^.");
      else throw new Error("The symbol " + c + " isn't a Boolean operator. Use ∧ ∨ ¬ ⊻ and brackets.");
    }
    if (!tokens.length) throw new Error("Type an expression first.");

    var position = 0;
    function peek() {
      return tokens[position];
    }
    function binary(next, symbol) {
      return function () {
        var left = next();
        while (peek() === symbol) {
          position++;
          left = { op: symbol, a: left, b: next() };
        }
        return left;
      };
    }
    function unary() {
      var token = peek();
      if (token === "¬") {
        position++;
        return { op: "¬", a: unary() };
      }
      if (token === "(") {
        position++;
        var inside = equivalence();
        if (peek() !== ")") throw new Error("A bracket ( has no ) to close it.");
        position++;
        return inside;
      }
      if (token === "0" || token === "1") {
        position++;
        return { op: "const", value: token === "1" };
      }
      if (token && /[A-Z]/.test(token)) {
        position++;
        return { op: "var", name: token };
      }
      throw new Error(token ? "Something is missing before " + token + "." : "The expression ends too soon.");
    }
    var and = binary(unary, "∧");
    var xor = binary(and, "⊻");
    var or = binary(xor, "∨");
    var equivalence = binary(or, "≡");

    var tree = equivalence();
    if (position < tokens.length) {
      var extra = tokens[position];
      throw new Error(
        extra === ")" ? "A bracket ) has no ( before it." : "Put an operator (∧ ∨ ⊻) before " + extra + "."
      );
    }
    return tree;
  }

  function variables(tree, found) {
    if (tree.op === "var") found[tree.name] = true;
    if (tree.a) variables(tree.a, found);
    if (tree.b) variables(tree.b, found);
    return found;
  }

  // How many operators (¬ ∧ ∨ ⊻ ≡) an expression uses: fewer means simpler.
  function operators(tree) {
    if (tree.op === "var" || tree.op === "const") return 0;
    return 1 + operators(tree.a) + (tree.b ? operators(tree.b) : 0);
  }

  function evaluate(tree, values) {
    switch (tree.op) {
      case "var": return values[tree.name];
      case "const": return tree.value;
      case "¬": return !evaluate(tree.a, values);
      case "∧": return evaluate(tree.a, values) && evaluate(tree.b, values);
      case "∨": return evaluate(tree.a, values) || evaluate(tree.b, values);
      case "⊻": return evaluate(tree.a, values) !== evaluate(tree.b, values);
      case "≡": return evaluate(tree.a, values) === evaluate(tree.b, values);
    }
  }

  // Two expressions are the same if they give the same output for every
  // combination of inputs: they have the same truth table.
  function sameTruthTable(first, second) {
    var names = Object.keys(variables(second, variables(first, {})));
    if (names.length > 10) return false;
    for (var row = 0; row < 1 << names.length; row++) {
      var values = {};
      names.forEach(function (name, i) {
        values[name] = ((row >> i) & 1) === 1;
      });
      if (evaluate(first, values) !== evaluate(second, values)) return false;
    }
    return true;
  }

  // ---------------------------------------------------------------------------
  // Marking one answer box
  // ---------------------------------------------------------------------------

  // Values: case, spaces and commas don't matter ("1001 0101" = "10010101").
  function asValue(text) {
    return text.trim().toLowerCase().replace(/[–−]/g, "-").replace(/[\s,]+/g, "").replace(/\.$/, "");
  }

  // Words: case and punctuation don't matter ("Real-time" = "real time").
  function asWords(text) {
    return text
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, " ")
      .trim()
      .replace(/^(the|a|an) /, "");
  }

  // Returns { status: "right" | "wrong" | "empty" | "unreadable", note }.
  function markBox(box, typed) {
    var type = box.getAttribute("data-type");
    var accept = box.getAttribute("data-accept").split("\n");
    if (!typed.trim()) return { status: "empty" };

    if (type === "expression") {
      // "Q = A ∧ B" or "S ≡ A ⊻ B": the letter before = names the output, so drop it.
      var label = box.querySelector("label").textContent.trim();
      var named = label.match(/^([A-Z])\s*≡$/);
      var expression = typed.replace(/^\s*[A-Za-z]\s*=\s*/, "");
      if (named) expression = expression.replace(new RegExp("^\\s*" + named[1] + "\\s*[=≡]\\s*"), "");
      var tree;
      try {
        tree = parseExpression(expression);
      } catch (error) {
        return { status: "unreadable", note: error.message };
      }
      var answer = parseExpression(accept[0]);
      if (!sameTruthTable(tree, answer)) return { status: "wrong" };
      if (box.hasAttribute("data-simplest") && operators(tree) > operators(answer)) {
        return { status: "wrong", note: "Your expression is equivalent, but it can be simplified further." };
      }
      return { status: "right" };
    }

    var normalise = type === "words" ? asWords : asValue;
    var mine = normalise(typed);
    var right = accept.some(function (option) {
      return normalise(option) === mine;
    });
    return { status: right ? "right" : "wrong" };
  }

  // ---------------------------------------------------------------------------
  // One spec point's page
  // ---------------------------------------------------------------------------

  var page = document.querySelector("[data-exam]");
  if (!page) return;
  var point = page.getAttribute("data-exam");
  var marksKey = "exam:" + point;
  var answersKey = "exam-answers:" + point;
  var score = page.querySelector("[data-exam-score]");
  var questions = page.querySelectorAll("[data-exam-q]");

  var TICK =
    '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="m7.5 12.5 3 3 6-7"/></svg>';
  var CROSS =
    '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="m8.5 8.5 7 7M15.5 8.5l-7 7"/></svg>';

  function showScore() {
    var marks = loadJSON(marksKey);
    var sum = totals(marks);
    if (score) {
      score.textContent =
        sum.questions === 0
          ? "No questions marked yet"
          : sum.mark + " / " + sum.of + " marks on " + sum.questions + " of " + plural(questions.length, "question") +
            " (" + (sum.of ? Math.round((sum.mark * 100) / sum.of) : 0) + "%)";
    }
    // Mark the question numbers you have done in the jump list.
    page.querySelectorAll("[data-jump]").forEach(function (link) {
      var number = link.getAttribute("data-jump");
      var entry = marks[number];
      link.classList.toggle("is-marked", !!entry);
      link.setAttribute(
        "aria-label",
        "Question " + number + (entry ? ", your mark " + entry.mark + " out of " + entry.of : "")
      );
    });
  }

  function setUpQuestion(question) {
    var number = question.getAttribute("data-exam-q");
    var total = question.querySelector("[data-question-total]");
    var parts = question.querySelectorAll("[data-part-marks]");

    // Save one part's entry (or remove it, if entry is null) and update the question's total.
    function savePart(label, entry) {
      var marks = loadJSON(marksKey);
      var saved = marks[number] && marks[number].parts ? marks[number] : { parts: {} };
      if (entry) saved.parts[label] = entry;
      else delete saved.parts[label];
      var labels = Object.keys(saved.parts);
      if (labels.length) {
        saved.mark = 0;
        saved.of = 0;
        labels.forEach(function (key) {
          saved.mark += saved.parts[key].mark;
          saved.of += saved.parts[key].of;
        });
        marks[number] = saved;
      } else {
        delete marks[number];
      }
      saveJSON(marksKey, marks);
      showTotal();
      showScore();
    }

    function savedPart(label) {
      var entry = loadJSON(marksKey)[number];
      return entry && entry.parts ? entry.parts[label] : null;
    }

    function showTotal() {
      if (!total) return;
      var entry = loadJSON(marksKey)[number];
      if (!entry) {
        total.textContent = "";
        return;
      }
      var done = entry.parts ? Object.keys(entry.parts).length : parts.length;
      total.textContent =
        done >= parts.length
          ? "Your mark for question " + number + ": " + entry.mark + " out of " + question.getAttribute("data-marks")
          : "Your marks so far: " + entry.mark + " out of " + entry.of + " (" + done + " of " + plural(parts.length, "part") + " marked)";
    }

    parts.forEach(function (part) {
      if (part.querySelector("[data-auto]")) setUpAutoPart(part, savePart, savedPart);
      else setUpTickedPart(part, savePart, savedPart);
    });
    showTotal();
  }

  function saveAnswer(label, value) {
    var answers = loadJSON(answersKey);
    var empty = Array.isArray(value)
      ? value.every(function (v) { return !v.trim(); })
      : !value.trim();
    if (empty) delete answers[label];
    else answers[label] = value;
    saveJSON(answersKey, answers);
  }

  function clamp(value, of) {
    return Math.max(0, Math.min(of, Math.round(Number(value))));
  }

  // A part you mark yourself, by ticking mark points or typing a number.
  function setUpTickedPart(part, savePart, savedPart) {
    var label = part.getAttribute("data-part");
    var of = Number(part.getAttribute("data-part-marks"));
    var input = part.querySelector("[data-self-mark]");
    var boxes = part.querySelectorAll("[data-point]");
    var textarea = part.querySelector("[data-answer]");

    // Each row gives at most its own marks; the part gives at most its marks.
    function markFromTicks() {
      var mark = 0;
      part.querySelectorAll(".ms-row").forEach(function (row) {
        var ticked = row.querySelectorAll("[data-point]:checked").length;
        var rowMarks = row.getAttribute("data-row-marks");
        mark += rowMarks === null ? ticked : Math.min(ticked, Number(rowMarks));
      });
      return Math.min(mark, of);
    }

    function ticked() {
      return Array.prototype.map.call(part.querySelectorAll("[data-point]:checked"), function (box) {
        return box.getAttribute("data-point");
      });
    }

    var saved = savedPart(label);
    if (saved) {
      (saved.ticks || []).forEach(function (key) {
        var box = part.querySelector('[data-point="' + key + '"]');
        if (box) box.checked = true;
      });
      input.value = saved.mark;
    }
    if (textarea) {
      textarea.value = loadJSON(answersKey)[label] || "";
      textarea.addEventListener("input", function () {
        saveAnswer(label, textarea.value);
      });
    }

    boxes.forEach(function (box) {
      box.addEventListener("change", function () {
        var keys = ticked();
        if (!keys.length) {
          input.value = "";
          savePart(label, null);
          return;
        }
        var mark = markFromTicks();
        input.value = mark;
        savePart(label, { mark: mark, of: of, ticks: keys });
      });
      // The whole mark point is a tap target, not just the small box.
      var item = box.closest(".ms-point");
      item.addEventListener("click", function (event) {
        if (event.target === box || event.target.closest("a")) return;
        if (window.getSelection && String(window.getSelection())) return; // selecting text
        box.click();
      });
    });

    input.addEventListener("input", function () {
      var value = input.value.trim();
      if (value === "" || isNaN(Number(value))) {
        var keys = ticked();
        savePart(label, keys.length ? { mark: markFromTicks(), of: of, ticks: keys } : null);
        return;
      }
      savePart(label, { mark: clamp(value, of), of: of, ticks: ticked(), changed: true });
    });
    // When you leave the box, show the mark as it was saved (e.g. 12 out of 8 becomes 8).
    input.addEventListener("change", function () {
      var entry = savedPart(label);
      input.value = entry ? entry.mark : "";
    });
  }

  // A part with one right answer, marked by this script.
  function setUpAutoPart(part, savePart, savedPart) {
    var label = part.getAttribute("data-part");
    var of = Number(part.getAttribute("data-part-marks"));
    var boxes = Array.prototype.slice.call(part.querySelectorAll("[data-box]"));
    var inputs = boxes.map(function (box) {
      return box.querySelector("[data-box-input]");
    });
    var check = part.querySelector("[data-check]");
    var change = part.querySelector("[data-change]");
    var verdict = part.querySelector("[data-verdict]");
    var markArea = part.querySelector("[data-part-mark]");
    var markInput = markArea.querySelector("[data-self-mark]");

    function values() {
      return inputs.map(function (input) {
        return input.value;
      });
    }

    function clearResults() {
      boxes.forEach(function (box) {
        box.querySelector("[data-box-result]").innerHTML = "";
        box.classList.remove("is-right", "is-wrong");
      });
    }

    // Marks every box and shows the result. Returns the mark, or null if an
    // answer is missing or can't be read (nothing is marked then).
    function run(quiet) {
      clearResults();
      var results = boxes.map(function (box, i) {
        return markBox(box, inputs[i].value);
      });
      var problem = null;
      results.forEach(function (result, i) {
        if (problem) return;
        if (result.status === "empty") problem = boxes.length > 1 ? "Type an answer in every box first." : "Type your answer first.";
        if (result.status === "unreadable") problem = "I couldn't read that: " + result.note;
        if (problem && !quiet) inputs[i].focus();
      });
      if (problem) {
        if (!quiet) verdict.innerHTML = '<p class="auto-problem">' + escapeHTML(problem) + "</p>";
        return null;
      }

      var mark = 0;
      var right = 0;
      var notes = [];
      results.forEach(function (result, i) {
        var ok = result.status === "right";
        var box = boxes[i];
        if (ok) {
          mark += Number(box.getAttribute("data-box-marks"));
          right++;
        }
        if (result.note) notes.push(result.note);
        box.classList.add(ok ? "is-right" : "is-wrong");
        if (boxes.length > 1) {
          box.querySelector("[data-box-result]").innerHTML =
            (ok ? TICK : CROSS) + '<span class="visually-hidden">' + (ok ? "Right" : "Not right") + "</span>";
        }
      });
      var all = right === results.length;
      var heading = all
        ? "Correct"
        : "Not quite" + (boxes.length > 1 && right > 0 ? ": " + right + " of " + boxes.length + " right" : "");
      verdict.innerHTML =
        '<div class="verdict ' + (all ? "is-right" : "is-wrong") + '">' + (all ? TICK : CROSS) +
        "<p>" + heading + "</p></div>" +
        notes.map(function (note) { return "<p>" + escapeHTML(note) + "</p>"; }).join("") +
        '<p class="auto-mark-line num" data-auto-mark-line></p>' +
        (all ? "" : '<p class="auto-hint">Open the mark scheme below to see the answer.</p>');
      return { mark: mark, status: all ? "right" : right ? "partly" : "wrong" };
    }

    function showMarkLine(entry) {
      var line = verdict.querySelector("[data-auto-mark-line]");
      if (line && entry) {
        line.textContent = "Your mark: " + entry.mark + " out of " + of + (entry.changed ? " (changed by you)" : "");
      }
      change.hidden = !entry;
    }

    // Put back what was saved last time.
    var savedValues = loadJSON(answersKey)[label];
    if (Array.isArray(savedValues)) {
      inputs.forEach(function (input, i) {
        input.value = savedValues[i] || "";
      });
    }
    var saved = savedPart(label);
    if (saved && saved.auto) {
      run(true);
      showMarkLine(saved);
      if (saved.changed) {
        markArea.hidden = false;
        markInput.value = saved.mark;
      }
    }

    inputs.forEach(function (input) {
      input.addEventListener("input", function () {
        saveAnswer(label, values());
      });
      input.addEventListener("keydown", function (event) {
        if (event.key === "Enter") {
          event.preventDefault();
          check.click();
        }
      });
    });

    // The ∧ ∨ ¬ ⊻ ( ) buttons type their symbol where the cursor is.
    boxes.forEach(function (box, i) {
      box.querySelectorAll("[data-symbol]").forEach(function (button) {
        button.addEventListener("click", function () {
          var input = inputs[i];
          var symbol = button.getAttribute("data-symbol");
          var start = input.selectionStart === null ? input.value.length : input.selectionStart;
          var end = input.selectionEnd === null ? start : input.selectionEnd;
          input.value = input.value.slice(0, start) + symbol + input.value.slice(end);
          input.focus();
          input.setSelectionRange(start + symbol.length, start + symbol.length);
          saveAnswer(label, values());
        });
      });
    });

    check.addEventListener("click", function () {
      var result = run(false);
      if (!result) return;
      var entry = { mark: result.mark, of: of, auto: result.status };
      savePart(label, entry);
      markArea.hidden = true;
      showMarkLine(entry);
      verdict.querySelector(".verdict").setAttribute("tabindex", "-1");
    });

    // If the marking got it wrong (or you showed working that earns marks),
    // set your own mark for this part.
    change.addEventListener("click", function () {
      var entry = savedPart(label);
      markArea.hidden = false;
      markInput.value = entry ? entry.mark : "";
      markInput.focus();
    });

    markInput.addEventListener("input", function () {
      var value = markInput.value.trim();
      if (value === "" || isNaN(Number(value))) return;
      var entry = savedPart(label) || { auto: "wrong" };
      entry = { mark: clamp(value, of), of: of, auto: entry.auto, changed: true };
      savePart(label, entry);
      showMarkLine(entry);
    });
    markInput.addEventListener("change", function () {
      var entry = savedPart(label);
      markInput.value = entry ? entry.mark : "";
    });
  }

  function escapeHTML(text) {
    return text.replace(/[&<>"]/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c];
    });
  }

  questions.forEach(setUpQuestion);

  var clear = page.querySelector("[data-exam-clear]");
  if (clear) {
    clear.addEventListener("click", function () {
      if (!window.confirm("Clear all your answers and marks for these exam questions?")) return;
      saveJSON(marksKey, {});
      saveJSON(answersKey, {});
      page.querySelectorAll("[data-self-mark], [data-answer], [data-box-input]").forEach(function (input) {
        input.value = "";
      });
      page.querySelectorAll("[data-point]").forEach(function (box) {
        box.checked = false;
      });
      page.querySelectorAll("[data-verdict], [data-box-result]").forEach(function (area) {
        area.innerHTML = "";
      });
      page.querySelectorAll("[data-box]").forEach(function (box) {
        box.classList.remove("is-right", "is-wrong");
      });
      page.querySelectorAll("[data-auto]").forEach(function (area) {
        var part = area.closest("[data-part]");
        part.querySelector("[data-part-mark]").hidden = true;
        part.querySelector("[data-change]").hidden = true;
      });
      page.querySelectorAll("[data-question-total]").forEach(function (total) {
        total.textContent = "";
      });
      showScore();
    });
  }

  showScore();
})();
