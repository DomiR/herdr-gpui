// Sheprd keeps what a page writes to its console, so an agent can read it
// with `sheprd browser logs`: console.debug, log, info, warn and error,
// errors nobody caught, and promise rejections nobody handled. It runs
// before the page's own scripts, in the main frame, and lasts as long as the
// document: a navigation starts an empty log. The newest MAX_ENTRIES are kept,
// each cut to MAX_TEXT characters; older ones are only counted.
(function () {
  "use strict";
  var key = Symbol.for("sheprd.console");
  if (window[key]) {
    return;
  }
  var MAX_ENTRIES = 500;
  var MAX_TEXT = 2000;
  var entries = [];
  var dropped = 0;
  function show(value) {
    if (typeof value === "string") {
      return value;
    }
    try {
      if (value instanceof Error) {
        return value.stack ? String(value) + "\n" + value.stack : String(value);
      }
      var json = JSON.stringify(value);
      if (json !== undefined) {
        return json;
      }
    } catch (_) {
      // Cycles and getters that throw fall through to String.
    }
    try {
      return String(value);
    } catch (_) {
      return "[a value that cannot be shown]";
    }
  }
  function keep(level, values) {
    try {
      var text = Array.prototype.map.call(values, show).join(" ");
      if (text.length > MAX_TEXT) {
        text = text.slice(0, MAX_TEXT) + "\u2026";
      }
      entries.push({ level: level, text: text, at: Date.now() });
      if (entries.length > MAX_ENTRIES) {
        entries.shift();
        dropped += 1;
      }
    } catch (_) {
      // Keeping a log must never break the page.
    }
  }
  ["debug", "log", "info", "warn", "error"].forEach(function (level) {
    var original = console[level];
    if (typeof original !== "function") {
      return;
    }
    console[level] = function () {
      keep(level, arguments);
      return original.apply(this, arguments);
    };
  });
  window.addEventListener("error", function (event) {
    keep("error", [event.error || event.message]);
  });
  window.addEventListener("unhandledrejection", function (event) {
    keep("error", ["Unhandled rejection:", event.reason]);
  });
  Object.defineProperty(window, key, {
    value: Object.freeze({
      read: function (clear) {
        var kept = { entries: entries.slice(), dropped: dropped };
        if (clear) {
          entries = [];
          dropped = 0;
        }
        return kept;
      },
    }),
  });
})();
