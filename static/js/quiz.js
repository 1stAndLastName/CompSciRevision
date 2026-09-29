// The quiz, run in the browser.
//
// The quiz page lists every question with its options, answer and explanation
// (that list is what shows without JavaScript). This script reads the list and
// shows one question at a time: questions and options are shuffled on every
// attempt (unless a question says shuffle = false), each answer gets a tick or
// cross plus "Correct" or "Not quite" and the explanation, and the summary
// saves the best score with window.Progress (see site.js).
//
// innerHTML is safe here: every piece of HTML comes from the page itself,
// rendered from our own content files, where raw HTML is escaped (see
// markdown_to_html in src/content.rs). Nothing comes from the user or the URL.

(function () {
  "use strict";

  var root = document.querySelector("[data-quiz]");
  var app = document.getElementById("quiz-app");
  if (!root || !app) return;

  var slug = root.getAttribute("data-quiz");
  var topicUrl = root.getAttribute("data-topic-url");

  // Read every question from the page.
  var questions = Array.prototype.map.call(root.querySelectorAll(".quiz-q"), function (item) {
    return {
      prompt: item.querySelector(".prompt").innerHTML,
      options: Array.prototype.map.call(item.querySelectorAll(".quiz-options > li"), function (option) {
        return option.innerHTML;
      }),
      answer: Number(item.getAttribute("data-answer")),
      shuffle: item.getAttribute("data-shuffle") !== "false",
      difficulty: Number(item.getAttribute("data-difficulty")),
      explanation: item.querySelector(".quiz-explanation").innerHTML,
    };
  });
  var total = questions.length;
  if (total === 0) return;

  var order = []; // question indexes, in the order asked
  var position = 0; // which question we are on (0-based)
  var score = 0;
  var optionOrder = []; // option indexes, in the order shown

  var TICK = '<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m4.5 10.5 3.5 3.5 7.5-8"/></svg>';
  var CROSS = '<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5.5 5.5 9 9M14.5 5.5l-9 9"/></svg>';

  // 0, 1, ... n-1 in a random order (Fisher-Yates shuffle).
  function shuffled(n) {
    var list = [];
    for (var i = 0; i < n; i++) list.push(i);
    for (var j = n - 1; j > 0; j--) {
      var k = Math.floor(Math.random() * (j + 1));
      var temp = list[j];
      list[j] = list[k];
      list[k] = temp;
    }
    return list;
  }

  function inOrder(n) {
    var list = [];
    for (var i = 0; i < n; i++) list.push(i);
    return list;
  }

  function status(answered) {
    return (
      '<div class="quiz-status"><p class="num">Question ' + (position + 1) + " of " + total + "</p>" +
      '<p class="num">Score ' + score + "</p></div>" +
      '<div class="meter" aria-hidden="true"><span style="width: ' + (answered * 100) / total + '%"></span></div>'
    );
  }

  // Show a step, then move keyboard focus to its heading so keyboard and
  // screen reader users carry on from the right place.
  function show(html) {
    app.innerHTML = html;
    var target = app.querySelector("[data-focus]");
    if (target) {
      target.focus({ preventScroll: true });
      if (app.getBoundingClientRect().top < 0) app.scrollIntoView({ block: "start" });
    }
  }

  function start() {
    order = shuffled(total);
    position = 0;
    score = 0;
    ask();
  }

  function ask() {
    var question = questions[order[position]];
    optionOrder = question.shuffle ? shuffled(question.options.length) : inOrder(question.options.length);
    var pips = "";
    for (var level = 1; level <= 3; level++) pips += level <= question.difficulty ? '<i class="on"></i>' : "<i></i>";
    var options = optionOrder
      .map(function (index) {
        return (
          '<label class="option"><input type="radio" name="choice" value="' + index + '" required>' +
          '<span class="option-text">' + question.options[index] + "</span></label>"
        );
      })
      .join("");
    show(
      '<section class="quiz-step" aria-label="Question ' + (position + 1) + " of " + total + '">' +
        status(position) +
        '<form class="quiz-form"><fieldset aria-labelledby="question-prompt">' +
        '<div class="question-head"><p class="difficulty" title="Difficulty ' + question.difficulty + ' of 3">' +
        '<span class="pips" aria-hidden="true">' + pips + "</span>" +
        '<span class="visually-hidden">Difficulty ' + question.difficulty + " of 3</span></p>" +
        '<div id="question-prompt" class="prompt" tabindex="-1" data-focus>' + question.prompt + "</div></div>" +
        '<div class="options">' + options + "</div></fieldset>" +
        '<button type="submit" class="button primary">Check answer</button></form></section>'
    );
    app.querySelector("form").addEventListener("submit", function (event) {
      event.preventDefault();
      var chosen = app.querySelector('input[name="choice"]:checked');
      if (chosen) feedback(Number(chosen.value));
    });
  }

  function feedback(choice) {
    var question = questions[order[position]];
    var correct = choice === question.answer;
    if (correct) score++;
    var verdict = correct
      ? '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="m7.5 12.5 3 3 6-7"/></svg><p>Correct</p>'
      : '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="m8.5 8.5 7 7M15.5 8.5l-7 7"/></svg><p>Not quite</p>';
    var results = optionOrder
      .map(function (index) {
        var isAnswer = index === question.answer;
        var isChosen = index === choice;
        var classes = "result" + (isAnswer ? " is-answer" : "") + (isChosen ? " is-chosen" : "");
        var icon = isAnswer ? TICK : isChosen ? CROSS : "";
        var note = isChosen && isAnswer ? "Your answer, correct" : isChosen ? "Your answer" : isAnswer ? "Correct answer" : "";
        return (
          '<li class="' + classes + '"><span class="result-icon" aria-hidden="true">' + icon + "</span>" +
          '<span class="option-text">' + question.options[index] + "</span>" +
          (note ? '<span class="result-note">' + note + "</span>" : "") + "</li>"
        );
      })
      .join("");
    var last = position === total - 1;
    show(
      '<section class="quiz-step" aria-label="Question ' + (position + 1) + " of " + total + ': feedback">' +
        status(position + 1) +
        '<div class="verdict ' + (correct ? "is-right" : "is-wrong") + '" tabindex="-1" data-focus>' + verdict + "</div>" +
        '<div class="prompt">' + question.prompt + "</div>" +
        '<ul class="results">' + results + "</ul>" +
        '<div class="explanation"><h2>Why</h2>' + question.explanation + "</div>" +
        '<button type="button" class="button primary" data-next>' + (last ? "See your score" : "Next question") + "</button>" +
        "</section>"
    );
    app.querySelector("[data-next]").addEventListener("click", function () {
      position++;
      if (position < total) ask();
      else summary();
    });
  }

  function summary() {
    var message =
      score === total
        ? "Full marks. Well done!"
        : score * 3 >= total * 2
          ? "Good work. Check the ones you missed and try again."
          : "Worth another look. Read the notes, then have another go.";
    var best = "";
    if (window.Progress) {
      var saved = window.Progress.saveQuiz(slug, score, total);
      if (saved.isNewBest && saved.hadBest) best = "New best score for this topic.";
      else if (!saved.isNewBest) best = "Your best: " + saved.best.score + "/" + saved.best.total;
    }
    show(
      '<section class="quiz-step summary">' +
        '<h2 tabindex="-1" data-focus>Your score</h2>' +
        '<p class="big-score num">' + score + "<span> / " + total + "</span></p>" +
        "<p>" + message + "</p>" +
        (best ? '<p class="best num">' + best + "</p>" : "") +
        '<div class="practice-actions">' +
        '<button type="button" class="button primary" data-again>Try again</button>' +
        '<a class="button" href="' + topicUrl + '/flashcards">Flashcards</a>' +
        '<a class="button quiet" href="' + topicUrl + '">Back to notes</a>' +
        "</div></section>"
    );
    app.querySelector("[data-again]").addEventListener("click", start);
  }

  start();
})();
