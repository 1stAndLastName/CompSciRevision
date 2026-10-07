// Draws the Mermaid diagrams on a page (the exam questions' redrawn diagrams).
//
// In the Markdown a diagram is a ```mermaid code block, which becomes
// <pre><code class="language-mermaid">. This script swaps each one for a
// drawing, keeping the code underneath in a "Diagram as text" section, which
// is also what screen readers use. Mermaid is large (about 5MB), so it is only
// loaded on pages that have a diagram. If it fails to load or a diagram will
// not draw, the code stays as it was.

(function () {
  "use strict";

  var blocks = document.querySelectorAll("pre > code.language-mermaid");
  if (!blocks.length) return;

  // Mermaid sits in static/vendor, next to static/js. Working it out from this
  // script's own address keeps it right both locally and on GitHub Pages.
  var here = document.currentScript && document.currentScript.src;
  if (!here) return;
  var mermaidSrc = new URL("../vendor/mermaid-12.0.0.min.js", here).href;

  var figures = [];
  blocks.forEach(function (code, i) {
    var pre = code.parentElement;
    var figure = document.createElement("figure");
    figure.className = "diagram";
    var drawing = document.createElement("div");
    drawing.className = "diagram-drawing";
    var details = document.createElement("details");
    details.className = "diagram-text";
    var summary = document.createElement("summary");
    summary.textContent = "Diagram as text";
    pre.replaceWith(figure);
    details.append(summary, pre);
    figure.append(drawing, details);
    figures.push({ id: "diagram-" + i, code: code.textContent, drawing: drawing, figure: figure });
  });

  // Mermaid needs real colours, so read the current theme's CSS variables.
  function colours() {
    var style = getComputedStyle(document.documentElement);
    function get(name) {
      return style.getPropertyValue(name).trim();
    }
    return {
      background: get("--surface"),
      primaryColor: get("--raised"),
      primaryBorderColor: get("--border-strong"),
      primaryTextColor: get("--text"),
      secondaryColor: get("--accent-soft"),
      tertiaryColor: get("--surface"),
      lineColor: get("--muted"),
      textColor: get("--text"),
      edgeLabelBackground: get("--surface"),
      fontFamily: "Inter, system-ui, sans-serif",
    };
  }

  // Draw every diagram, one at a time (Mermaid can only draw one at once).
  function drawAll() {
    window.mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "base",
      themeVariables: colours(),
    });
    return figures.reduce(function (previous, item) {
      return previous.then(function () {
        return window.mermaid
          .render(item.id, item.code)
          .then(function (result) {
            // Safe to insert: in "strict" mode Mermaid cleans its SVG with DOMPurify.
            item.drawing.innerHTML = result.svg;
            var svg = item.drawing.querySelector("svg");
            if (svg) {
              svg.setAttribute("role", "img");
              svg.setAttribute("aria-label", "Diagram. A text version follows.");
              // Mermaid shrinks a diagram to fit its box. On a phone that can make
              // the labels too small to read, so stop at 75% and scroll instead.
              var width = svg.viewBox && svg.viewBox.baseVal ? svg.viewBox.baseVal.width : 0;
              if (width) svg.style.minWidth = Math.round(width * 0.75) + "px";
            }
            item.figure.classList.add("is-drawn");
          })
          .catch(function () {
            // Leave the code showing: open the text version instead.
            item.figure.classList.remove("is-drawn");
            item.figure.querySelector("details").open = true;
            var stray = document.getElementById("d" + item.id);
            if (stray) stray.remove();
          });
      });
    }, Promise.resolve());
  }

  var script = document.createElement("script");
  script.src = mermaidSrc;
  script.onload = function () {
    drawAll().then(function () {
      // Redraw in the new colours when the theme is changed.
      new MutationObserver(drawAll).observe(document.documentElement, {
        attributes: true,
        attributeFilter: ["data-theme"],
      });
    });
  };
  script.onerror = function () {
    figures.forEach(function (item) {
      item.figure.querySelector("details").open = true;
    });
  };
  document.head.appendChild(script);
})();
