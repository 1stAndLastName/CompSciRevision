// Flashcards: one card at a time, flip, previous/next, shuffle, Focus, and
// "Got it" / "Again" marks saved with window.Progress (see site.js).
//
// Order: cards marked "Again" first, then cards not yet seen, then "Got it"
// cards last. Shuffle mixes the cards within each of those groups.
// Focus hides cards already marked "Got it".

(function () {
  "use strict";

  var root = document.querySelector("[data-flashcards]");
  if (!root || !window.Progress) return;

  var slug = root.getAttribute("data-flashcards");
  var deck = root.querySelector(".fc-deck");
  var cards = Array.prototype.slice.call(deck.querySelectorAll(".fc-card"));
  var cellRow = root.querySelector(".fc-cells .cells");
  var cellFor = {};
  cellRow.querySelectorAll("i[data-card]").forEach(function (cell) {
    cellFor[cell.getAttribute("data-card")] = cell;
  });

  var positionText = root.querySelector("[data-position]");
  var chip = root.querySelector("[data-chip]");
  var doneBox = root.querySelector("[data-done]");
  var doneSummary = root.querySelector("[data-done-summary]");
  var announcer = root.querySelector("[data-announce]");
  var prevButton = root.querySelector("[data-prev]");
  var nextButton = root.querySelector("[data-next]");
  var flipButton = root.querySelector("[data-flip]");
  var markButtons = root.querySelectorAll("[data-mark]");
  var focusToggle = root.querySelector("[data-focus-toggle]");

  var data = Progress.load(slug);
  var focusMode = Store.read("focus") === "1";
  var order = []; // every card, in the order they are dealt
  var visible = []; // the cards being gone through (fewer in Focus mode)
  var index = 0; // position in `visible`; visible.length means "end of deck"
  var flipped = false;

  function idOf(card) {
    return card.getAttribute("data-card");
  }

  function statusOf(card) {
    return data.cards[idOf(card)] || "";
  }

  // Again = 0, not seen = 1, got it = 2.
  function groupOf(card) {
    var status = statusOf(card);
    if (status === "again") return 0;
    if (status === "got") return 2;
    return 1;
  }

  // Sort by group, keeping the current order within each group.
  function sortByStatus(list) {
    return list
      .map(function (card, i) {
        return { card: card, i: i };
      })
      .sort(function (a, b) {
        return groupOf(a.card) - groupOf(b.card) || a.i - b.i;
      })
      .map(function (item) {
        return item.card;
      });
  }

  // Fisher-Yates shuffle on a copy of the list.
  function shuffled(list) {
    var copy = list.slice();
    for (var i = copy.length - 1; i > 0; i--) {
      var j = Math.floor(Math.random() * (i + 1));
      var temp = copy[i];
      copy[i] = copy[j];
      copy[j] = temp;
    }
    return copy;
  }

  function deal(list) {
    order = list;
    // Move the cards and cells in the page into the new order.
    order.forEach(function (card) {
      deck.appendChild(card);
      cellRow.appendChild(cellFor[idOf(card)]);
    });
    updateVisible();
  }

  function updateVisible() {
    visible = focusMode
      ? order.filter(function (card) {
          return statusOf(card) !== "got";
        })
      : order.slice();
  }

  function currentCard() {
    return index < visible.length ? visible[index] : null;
  }

  function goTo(newIndex) {
    index = Math.max(0, Math.min(newIndex, visible.length));
    flipped = false;
    render();
  }

  function render() {
    var card = currentCard();
    var atEnd = card === null;

    cards.forEach(function (c) {
      var isCurrent = c === card;
      c.classList.toggle("is-current", isCurrent);
      c.classList.toggle("is-flipped", isCurrent && flipped);
      // Only the side that is showing is read out by screen readers.
      c.querySelector(".fc-front").setAttribute("aria-hidden", String(!isCurrent || flipped));
      c.querySelector(".fc-back").setAttribute("aria-hidden", String(!isCurrent || !flipped));
    });

    // Cells: saved marks, plus an outline on the current card.
    cards.forEach(function (c) {
      var cell = cellFor[idOf(c)];
      var status = statusOf(c);
      cell.classList.toggle("got", status === "got");
      cell.classList.toggle("again", status === "again");
      cell.classList.toggle("current", c === card);
    });

    if (atEnd) {
      positionText.textContent = visible.length ? "End of the deck" : "No cards to show";
      chip.hidden = true;
    } else {
      positionText.textContent = "Card " + (index + 1) + " of " + visible.length;
      var status = statusOf(card);
      chip.hidden = status === "";
      chip.className = "fc-chip " + status;
      chip.textContent = status === "got" ? "Got it" : "Again";
    }

    doneBox.hidden = !atEnd;
    if (atEnd) doneSummary.textContent = summaryText();

    prevButton.disabled = index === 0;
    nextButton.disabled = atEnd;
    flipButton.disabled = atEnd;
    markButtons.forEach(function (button) {
      button.disabled = atEnd;
    });
    flipButton.textContent = flipped ? "Show question" : "Show answer";
    flipButton.setAttribute("aria-pressed", String(flipped));
  }

  function summaryText() {
    var got = 0;
    var again = 0;
    order.forEach(function (card) {
      if (statusOf(card) === "got") got++;
      if (statusOf(card) === "again") again++;
    });
    if (focusMode && visible.length === 0) {
      return "Every card is marked Got it. Turn off Focus to go through them all again.";
    }
    var notSeen = order.length - got - again;
    return got + " got it, " + again + " to go over again, " + notSeen + " not seen yet.";
  }

  function announce(text) {
    announcer.textContent = "";
    // A short delay makes screen readers notice the change.
    setTimeout(function () {
      announcer.textContent = text;
    }, 50);
  }

  function flip() {
    var card = currentCard();
    if (!card) return;
    flipped = !flipped;
    render();
    var side = flipped ? ".fc-back .fc-text" : ".fc-front .fc-text";
    announce((flipped ? "Answer: " : "Question: ") + card.querySelector(side).textContent.trim());
  }

  function mark(status) {
    var card = currentCard();
    if (!card) return;
    data = Progress.setCard(slug, idOf(card), status);
    announce(status === "got" ? "Marked Got it." : "Marked Again.");
    if (focusMode && status === "got") {
      // The card leaves the Focus deck, so the same index is now the next card.
      updateVisible();
      goTo(index);
    } else {
      goTo(index + 1);
    }
    if (!currentCard()) doneBox.focus();
  }

  function next() {
    goTo(index + 1);
    if (!currentCard()) doneBox.focus();
  }

  function restart() {
    data = Progress.load(slug);
    deal(sortByStatus(cards));
    goTo(0);
    flipButton.focus();
  }

  // --- Wire up the controls -------------------------------------------------

  flipButton.addEventListener("click", flip);
  prevButton.addEventListener("click", function () {
    goTo(index - 1);
  });
  nextButton.addEventListener("click", next);
  markButtons.forEach(function (button) {
    button.addEventListener("click", function () {
      mark(button.getAttribute("data-mark"));
    });
  });

  root.querySelector("[data-shuffle]").addEventListener("click", function () {
    // Shuffle first, then sort by group: the groups stay in order but the
    // cards inside each group are mixed up.
    deal(sortByStatus(shuffled(cards)));
    goTo(0);
    announce("Cards shuffled.");
  });

  root.querySelector("[data-restart]").addEventListener("click", restart);

  focusToggle.checked = focusMode;
  focusToggle.addEventListener("change", function () {
    focusMode = focusToggle.checked;
    Store.write("focus", focusMode ? "1" : "0");
    var card = currentCard();
    updateVisible();
    var stillThere = card ? visible.indexOf(card) : -1;
    goTo(stillThere >= 0 ? stillThere : 0);
  });

  // Clicking or tapping the card itself flips it (unless text is being selected).
  deck.addEventListener("click", function (event) {
    var card = event.target.closest(".fc-card");
    if (!card || card !== currentCard()) return;
    if (window.getSelection && String(window.getSelection()).length > 0) return;
    flip();
  });

  // Keys: Space flips, left and right arrows move. They are ignored while
  // typing or when another control (a button, link or radio) has focus, so
  // those controls keep their normal keyboard behaviour.
  document.addEventListener("keydown", function (event) {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    var tag = event.target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
    var onControl = tag === "BUTTON" || tag === "A";

    if (event.key === " " && !onControl) {
      event.preventDefault();
      flip();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      if (currentCard()) next();
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      goTo(index - 1);
    }
  });

  deal(sortByStatus(cards));
  goTo(0);
})();
