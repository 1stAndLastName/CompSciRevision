// Exam questions: the marks students give themselves, saved in this browser.
//
// On a spec point's exam page, each question has a box for your own mark,
// filled in after reading the mark scheme. The marks are saved in
// localStorage (see Store in site.js) under "h446:exam:<spec point>" as
//   { "<question number>": { "mark": 5, "of": 8 } }
// The exam questions list shows each spec point's total from the same data.

(function () {
  "use strict";

  if (!window.Store) return;

  function load(point) {
    var data = null;
    try {
      data = JSON.parse(window.Store.read("exam:" + point));
    } catch (e) {
      data = null;
    }
    return data && typeof data === "object" ? data : {};
  }

  function save(point, marks) {
    window.Store.write("exam:" + point, Object.keys(marks).length ? JSON.stringify(marks) : null);
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
    var sum = totals(load(row.getAttribute("data-exam-progress")));
    if (text && sum.questions > 0) {
      text.textContent = "You: " + sum.mark + "/" + sum.of + " on " + plural(sum.questions, "question");
    }
  });

  // One spec point's page.
  var page = document.querySelector("[data-exam]");
  if (!page) return;
  var point = page.getAttribute("data-exam");
  var score = page.querySelector("[data-exam-score]");
  var questions = page.querySelectorAll("[data-exam-q]");

  function showScore() {
    var marks = load(point);
    var sum = totals(marks);
    if (score) {
      score.textContent =
        sum.questions === 0
          ? "No questions marked yet"
          : sum.mark + " / " + sum.of + " marks on " + sum.questions + " of " + plural(questions.length, "question") +
            " (" + Math.round((sum.mark * 100) / sum.of) + "%)";
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

  questions.forEach(function (question) {
    var number = question.getAttribute("data-exam-q");
    var of = Number(question.getAttribute("data-marks"));
    var input = question.querySelector("[data-self-mark]");
    if (!input) return;
    var saved = load(point)[number];
    if (saved) input.value = saved.mark;

    function store() {
      var marks = load(point);
      var value = input.value.trim();
      if (value === "" || isNaN(Number(value))) {
        delete marks[number];
      } else {
        var mark = Math.max(0, Math.min(of, Math.round(Number(value))));
        marks[number] = { mark: mark, of: of };
      }
      save(point, marks);
      showScore();
    }

    input.addEventListener("input", store);
    // When you leave the box, show the mark as it was saved (e.g. 12 out of 8 becomes 8).
    input.addEventListener("change", function () {
      var entry = load(point)[number];
      input.value = entry ? entry.mark : "";
    });
  });

  var clear = page.querySelector("[data-exam-clear]");
  if (clear) {
    clear.addEventListener("click", function () {
      if (!window.confirm("Clear all your marks for these exam questions?")) return;
      save(point, {});
      page.querySelectorAll("[data-self-mark]").forEach(function (input) {
        input.value = "";
      });
      showScore();
    });
  }

  showScore();
})();
