// Shared JavaScript for every page:
// - the theme picker
// - saving and showing progress (kept in this browser's localStorage only)
//
// Everything in localStorage starts with "h446:". Each topic's progress is one
// JSON value under "h446:progress:<topic-slug>":
//   { "cards": { "<card id>": "got" | "again" }, "quizBest": { "score": 5, "total": 6 } }

(function () {
  "use strict";

  var PREFIX = "h446:";

  // localStorage can be missing or blocked (private browsing, school
  // policies), so every read and write is wrapped in try/catch.
  var Store = {
    read: function (key) {
      try {
        return localStorage.getItem(PREFIX + key);
      } catch (e) {
        return null;
      }
    },
    write: function (key, value) {
      try {
        if (value === null) {
          localStorage.removeItem(PREFIX + key);
        } else {
          localStorage.setItem(PREFIX + key, value);
        }
      } catch (e) {
        // Progress just won't be remembered.
      }
    },
  };

  var Progress = {
    load: function (slug) {
      var data = null;
      try {
        data = JSON.parse(Store.read("progress:" + slug));
      } catch (e) {
        data = null;
      }
      if (!data || typeof data !== "object") data = {};
      if (!data.cards || typeof data.cards !== "object") data.cards = {};
      return data;
    },
    save: function (slug, data) {
      Store.write("progress:" + slug, JSON.stringify(data));
    },
    reset: function (slug) {
      Store.write("progress:" + slug, null);
    },
    setCard: function (slug, cardId, status) {
      var data = Progress.load(slug);
      data.cards[cardId] = status;
      Progress.save(slug, data);
      return data;
    },
    // Saves the score if it beats the best so far. Compares fractions, so a
    // best score still makes sense if questions are added to the topic later.
    saveQuiz: function (slug, score, total) {
      var data = Progress.load(slug);
      var best = data.quizBest;
      var isNewBest = !best || score / total > best.score / best.total;
      if (isNewBest) {
        data.quizBest = { score: score, total: total };
        Progress.save(slug, data);
      }
      return { isNewBest: isNewBest, best: data.quizBest, hadBest: !!best };
    },
  };

  // Fill in a progress area: its cells (one per card) and its summary text.
  function showProgress(area) {
    var slug = area.getAttribute("data-topic-progress");
    var data = Progress.load(slug);
    var cells = area.querySelectorAll(".cells i[data-card]");
    var got = 0;
    var again = 0;
    cells.forEach(function (cell) {
      var status = data.cards[cell.getAttribute("data-card")];
      cell.classList.toggle("got", status === "got");
      cell.classList.toggle("again", status === "again");
      if (status === "got") got++;
      if (status === "again") again++;
    });

    var cardsText = area.querySelector("[data-cards-text]");
    var quizText = area.querySelector("[data-quiz-text]");
    var best = data.quizBest;
    if (cardsText) {
      if (got === 0 && again === 0 && !best) {
        cardsText.textContent = "Not started";
      } else {
        cardsText.textContent = got + " of " + cells.length + " cards got it";
      }
    }
    if (quizText) {
      quizText.textContent = best ? "Best quiz " + best.score + "/" + best.total : "";
    }
  }

  function setUpThemePicker() {
    var current = document.documentElement.getAttribute("data-theme") || "dark";
    document.querySelectorAll('.theme-picker input[name="theme"]').forEach(function (input) {
      input.checked = input.value === current;
      input.addEventListener("change", function () {
        document.documentElement.setAttribute("data-theme", input.value);
        Store.write("theme", input.value);
        // Match the phone's browser bar to the header colour of the new theme.
        var barColour = { dark: "#121c18", oled: "#000000", light: "#ffffff" }[input.value];
        var meta = document.querySelector('meta[name="theme-color"]');
        if (meta && barColour) meta.content = barColour;
      });
    });
  }

  function setUpResetButtons() {
    document.querySelectorAll("[data-reset]").forEach(function (button) {
      button.addEventListener("click", function () {
        var ok = window.confirm(
          "Reset your progress for this topic? This clears your flashcard marks and best quiz score."
        );
        if (!ok) return;
        Progress.reset(button.getAttribute("data-reset"));
        document.querySelectorAll("[data-topic-progress]").forEach(showProgress);
        button.textContent = "Progress reset";
        setTimeout(function () {
          button.textContent = "Reset progress";
        }, 2500);
      });
    });
  }

  window.Store = Store;
  window.Progress = Progress;

  setUpThemePicker();
  setUpResetButtons();
  document.querySelectorAll("[data-topic-progress]").forEach(showProgress);
})();
