// Herdr UI's annotation picker. The app evaluates this in the page's main
// frame when annotating starts, passing the numbered notes already queued.
// It only reports what the user picks: an element, a text selection, a
// region drawn over the page, or a pen stroke drawn while it is recorded. The notes themselves are written in the app,
// where the page cannot read them. Everything it posts is treated as
// untrusted by the app, and nothing reaches an agent until the user sends.
(function (markers) {
  "use strict";
  var previous = window.__herdrAnnotate;
  // Markers already on screen before this run keep still; only a newly
  // numbered one pops in.
  var popped = previous && typeof previous.popped === "number" ? previous.popped : 0;
  if (previous && typeof previous.disarm === "function") {
    try { previous.disarm(); } catch (_) {}
  }
  var post = function (message) {
    try { window.ipc.postMessage(JSON.stringify(message)); } catch (_) {}
  };

  var host = document.createElement("div");
  host.style.cssText =
    "position:fixed;inset:0;z-index:2147483647;pointer-events:none;margin:0;padding:0;border:0";
  var root = host.attachShadow({ mode: "closed" });
  // A constant skeleton: page text only ever goes in through textContent.
  root.innerHTML =
    "<style>" +
    ".box{position:fixed;border:2px solid #0d99ff;background:rgba(13,153,255,.08);border-radius:2px;display:none}" +
    ".box.region{border-style:dashed;background:rgba(13,153,255,.12)}" +
    ".label{position:fixed;background:#0d99ff;color:#fff;font:11px/16px -apple-system,system-ui,sans-serif;padding:0 6px;border-radius:3px;display:none;white-space:nowrap}" +
    ".mark{position:fixed;border:2px dashed #f59e0b;border-radius:2px}" +
    ".pin{position:fixed;min-width:18px;height:18px;padding:0 4px;box-sizing:border-box;border-radius:9px;background:#f59e0b;color:#111;font:bold 11px/18px -apple-system,system-ui,sans-serif;text-align:center;box-shadow:0 1px 3px rgba(0,0,0,.4)}" +
    "@keyframes herdr-pop{from{transform:scale(.2);opacity:0}to{transform:scale(1);opacity:1}}" +
    "@keyframes herdr-fade{from{opacity:0}to{opacity:1}}" +
    "@media (prefers-reduced-motion:no-preference){.pin.new{animation:herdr-pop .26s cubic-bezier(.2,1.5,.4,1)}.mark.new{animation:herdr-fade .22s ease-out}}" +
    ".ink{position:fixed;left:0;top:0;pointer-events:none}" +
    "</style><div class=box></div><div class=label></div><div class=marks></div><canvas class=ink></canvas>";
  var box = root.querySelector(".box");
  var label = root.querySelector(".label");
  var marks = root.querySelector(".marks");
  var ink = root.querySelector(".ink");
  var inkContext = ink.getContext("2d");
  document.documentElement.appendChild(host);

  var ownsNode = function (node) {
    return node === host || host.contains(node);
  };

  // A path of tag:nth-of-type steps from <body>, or from the nearest
  // element with a unique id.
  var selectorOf = function (element) {
    var steps = [];
    for (var node = element; node && node.nodeType === 1 && node !== document.documentElement; node = node.parentElement) {
      if (node.id && window.CSS && CSS.escape) {
        var byId = "#" + CSS.escape(node.id);
        try {
          if (document.querySelectorAll(byId).length === 1) {
            steps.unshift(byId);
            break;
          }
        } catch (_) {}
      }
      var tag = node.tagName.toLowerCase();
      if (node === document.body) {
        steps.unshift("body");
        break;
      }
      var index = 1;
      for (var sibling = node.previousElementSibling; sibling; sibling = sibling.previousElementSibling) {
        if (sibling.tagName === node.tagName) index += 1;
      }
      steps.unshift(tag + ":nth-of-type(" + index + ")");
    }
    return steps.join(" > ");
  };

  var textOf = function (element, limit) {
    return String(element.innerText || element.textContent || "").replace(/\s+/g, " ").trim().slice(0, limit);
  };

  // The visible part of a rectangle, which is what a screenshot can show.
  var visible = function (rect) {
    var left = Math.max(0, rect.left);
    var top = Math.max(0, rect.top);
    var right = Math.min(window.innerWidth, rect.right);
    var bottom = Math.min(window.innerHeight, rect.bottom);
    if (right - left < 1 || bottom - top < 1) return null;
    return { x: left, y: top, width: right - left, height: bottom - top };
  };

  var elementAt = function (x, y) {
    var element = document.elementFromPoint(x, y);
    return element && !ownsNode(element) ? element : null;
  };

  var place = function (node, left, top, width, height) {
    node.style.left = left + "px";
    node.style.top = top + "px";
    if (width !== undefined) node.style.width = width + "px";
    if (height !== undefined) node.style.height = height + "px";
  };

  var hovered = null;
  var showBox = function (element) {
    hovered = element;
    box.className = "box";
    if (!element) {
      box.style.display = "none";
      label.style.display = "none";
      return;
    }
    var rect = element.getBoundingClientRect();
    place(box, rect.left, rect.top, rect.width, rect.height);
    box.style.display = "block";
    label.textContent = element.tagName.toLowerCase() + "  " + Math.round(rect.width) + "×" + Math.round(rect.height);
    place(label, Math.max(0, rect.left), rect.top >= 20 ? rect.top - 18 : rect.bottom + 2);
    label.style.display = "block";
  };

  var drawMarkers = function () {
    marks.textContent = "";
    var highest = 0;
    markers.forEach(function (marker) {
      var fresh = marker.number > popped;
      highest = Math.max(highest, marker.number);
      var rect = null;
      if (marker.rect) {
        rect = {
          left: marker.rect.x - window.scrollX,
          top: marker.rect.y - window.scrollY,
          width: marker.rect.width,
          height: marker.rect.height,
        };
      } else {
        var element = null;
        try { element = marker.selector ? document.querySelector(marker.selector) : null; } catch (_) {}
        if (!element) return;
        rect = element.getBoundingClientRect();
      }
      var mark = document.createElement("div");
      mark.className = fresh ? "mark new" : "mark";
      place(mark, rect.left, rect.top, rect.width, rect.height);
      var pin = document.createElement("div");
      pin.className = fresh ? "pin new" : "pin";
      pin.textContent = String(marker.number);
      place(pin, Math.max(0, rect.left - 9), Math.max(0, rect.top - 9));
      marks.appendChild(mark);
      marks.appendChild(pin);
    });
    // Sent or cleared notes number again from one, and pop again.
    popped = highest;
  };
  var frame = 0;
  var redraw = function () {
    if (frame) return;
    frame = requestAnimationFrame(function () {
      frame = 0;
      drawMarkers();
      if (hovered) showBox(hovered);
    });
  };

  // The overlay stays out of the screenshot the app takes of a pick, and
  // comes back when the app says so, or on its own if it never does.
  var hideTimer = 0;
  var conceal = function () {
    host.style.visibility = "hidden";
    clearTimeout(hideTimer);
    hideTimer = setTimeout(function () { host.style.visibility = ""; }, 2000);
  };
  var reveal = function () {
    clearTimeout(hideTimer);
    host.style.visibility = "";
  };

  // What a region covers: the topmost element at points across it, their
  // closest shared container, and their text.
  var covering = function (rect) {
    var seen = [];
    var steps = 5;
    for (var row = 0; row < steps; row += 1) {
      for (var column = 0; column < steps; column += 1) {
        var x = rect.x + (rect.width * (column + 0.5)) / steps;
        var y = rect.y + (rect.height * (row + 0.5)) / steps;
        var element = elementAt(x, y);
        if (element && element !== document.documentElement && seen.indexOf(element) < 0) seen.push(element);
      }
    }
    var container = seen[0] || document.body;
    var containsAll = function (candidate) {
      return seen.every(function (element) { return candidate.contains(element); });
    };
    while (container && container !== document.body && !containsAll(container)) {
      container = container.parentElement;
    }
    var texts = [];
    var covers = seen.slice(0, 8).map(function (element) {
      var text = textOf(element, 120);
      if (text && texts.indexOf(text) < 0) texts.push(text);
      return { selector: selectorOf(element), tag: element.tagName.toLowerCase(), text: text };
    });
    return {
      container: container ? selectorOf(container) : "body",
      covers: covers,
      text: texts.join(" | ").slice(0, 600),
    };
  };

  // What drew an element, when the page's dev build says: a source locator
  // some setups add, else React's fiber with its component and source. This
  // follows Scrawl's sdk.js (MIT, Kit Langton).
  var ignoredSource = /\/node_modules\/|\/\.vite\/deps\/|react-dom|jsx-dev-runtime|jsx-runtime/;
  var stackSource = function (stack) {
    var lines = String(stack || "").split("\n");
    for (var i = 0; i < lines.length; i += 1) {
      var match = lines[i].match(/((?:https?|file):\/\/[^\s()]+?):(\d+):\d+\)?\s*$/);
      if (!match || ignoredSource.test(match[1])) continue;
      try {
        return decodeURIComponent(new URL(match[1]).pathname).replace(/^\/(@fs\/)?/, "") + ":" + match[2];
      } catch (_) {}
    }
    return undefined;
  };
  var shortSource = function (file, lineNumber) {
    try {
      return decodeURIComponent(new URL(file, location.href).pathname).replace(/^\//, "") + (lineNumber ? ":" + lineNumber : "");
    } catch (_) {
      return undefined;
    }
  };
  var located = function (element) {
    var data = element.dataset || {};
    if (data.sourceLoc) return { source: data.sourceLoc.replace(/^(.*:\d+):\d+$/, "$1") };
    if (data.inspectorRelativePath) {
      return { source: data.inspectorRelativePath + (data.inspectorLine ? ":" + data.inspectorLine : "") };
    }
    if (data.sentrySourceFile) return { component: data.sentryComponent, source: data.sentrySourceFile };
    return null;
  };
  var fiberOf = function (element) {
    for (var key in element) {
      if (key.indexOf("__reactFiber$") === 0 || key.indexOf("__reactInternalInstance$") === 0) return element[key];
    }
    return null;
  };
  var reactInfo = function (fiber) {
    var out = {};
    var debugSource = fiber._debugSource;
    if (debugSource && debugSource.fileName) out.source = shortSource(debugSource.fileName, debugSource.lineNumber);
    else if (fiber._debugStack) out.source = stackSource(fiber._debugStack.stack);
    for (var node = fiber, depth = 0; node && depth < 200; node = node.return, depth += 1) {
      var type = node.type;
      var fn = typeof type === "function" ? type : type && typeof type === "object" ? type.render || type.type : null;
      var name = fn && (fn.displayName || fn.name);
      if (name) {
        out.component = String(name);
        break;
      }
    }
    return out;
  };
  var drawnBy = function (element) {
    try {
      for (var node = element, depth = 0; node && depth < 30; node = node.parentElement, depth += 1) {
        var found = located(node);
        if (found) return found;
        var fiber = fiberOf(node);
        if (fiber) return reactInfo(fiber);
      }
    } catch (_) {}
    return {};
  };
  var controls = "button, a, input, select, textarea, label, summary, [role=button], [role=tab], [role=menuitem], [role=link]";
  var describe = function (element) {
    var name = element.getAttribute("aria-label") || element.getAttribute("title") || element.getAttribute("alt");
    var described = {
      tag: element.tagName.toLowerCase(),
      selector: selectorOf(element),
      text: String(name || textOf(element, 80)).slice(0, 80),
    };
    var by = drawnBy(element);
    if (by.component) described.component = String(by.component).slice(0, 80);
    if (by.source) described.source = String(by.source).slice(0, 200);
    return described;
  };

  // The pen: strokes drawn over the page while it is recorded, with Shift
  // held, fading a moment after the pen lifts. Without Shift the page works
  // as it does unrecorded, so it can be clicked through while talking. Each finished stroke is posted with its
  // points, their times from its start, and the elements under it: across
  // the area it encloses when it closes on itself, else along its path.
  var MAX_POINTS = 2000;
  var stroke = null;
  var fadeTimers = [];
  var sizeInk = function () {
    var ratio = window.devicePixelRatio || 1;
    ink.width = Math.round(window.innerWidth * ratio);
    ink.height = Math.round(window.innerHeight * ratio);
    ink.style.width = window.innerWidth + "px";
    ink.style.height = window.innerHeight + "px";
    inkContext.setTransform(ratio, 0, 0, ratio, 0, 0);
  };
  sizeInk();
  var clearFade = function () {
    fadeTimers.forEach(clearTimeout);
    fadeTimers = [];
    ink.style.transition = "";
    ink.style.opacity = "1";
  };
  var fadeInk = function () {
    fadeTimers.push(setTimeout(function () {
      ink.style.transition = "opacity .3s ease-out";
      ink.style.opacity = "0";
      fadeTimers.push(setTimeout(function () {
        inkContext.clearRect(0, 0, window.innerWidth, window.innerHeight);
        clearFade();
      }, 320));
    }, 1500));
  };
  var inkSegment = function (from, to) {
    inkContext.strokeStyle = "#ff3b30";
    inkContext.lineWidth = 4;
    inkContext.lineCap = "round";
    inkContext.lineJoin = "round";
    inkContext.beginPath();
    inkContext.moveTo(from[0], from[1]);
    inkContext.lineTo(to[0], to[1]);
    inkContext.stroke();
  };
  var startStroke = function (event) {
    if (stroke) return;
    clearFade();
    stroke = { started: performance.now(), points: [[event.clientX, event.clientY, 0]] };
    inkSegment(stroke.points[0], stroke.points[0]);
  };
  var extendStroke = function (event) {
    var last = stroke.points[stroke.points.length - 1];
    var dx = event.clientX - last[0];
    var dy = event.clientY - last[1];
    if (dx * dx + dy * dy < 4 || stroke.points.length >= MAX_POINTS) return;
    var point = [event.clientX, event.clientY, Math.round(performance.now() - stroke.started)];
    inkSegment(last, point);
    stroke.points.push(point);
  };
  var underStroke = function (points) {
    var left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
    points.forEach(function (point) {
      left = Math.min(left, point[0]);
      top = Math.min(top, point[1]);
      right = Math.max(right, point[0]);
      bottom = Math.max(bottom, point[1]);
    });
    var diagonal = Math.sqrt((right - left) * (right - left) + (bottom - top) * (bottom - top));
    var first = points[0];
    var last = points[points.length - 1];
    var gap = Math.sqrt((last[0] - first[0]) * (last[0] - first[0]) + (last[1] - first[1]) * (last[1] - first[1]));
    var samples = [];
    if (points.length > 8 && diagonal > 24 && gap < diagonal * 0.3) {
      for (var row = 0; row < 5; row += 1) {
        for (var column = 0; column < 5; column += 1) {
          samples.push([left + ((right - left) * (column + 0.5)) / 5, top + ((bottom - top) * (row + 0.5)) / 5]);
        }
      }
    } else {
      var step = Math.max(1, Math.floor(points.length / 12));
      for (var index = 0; index < points.length; index += step) samples.push(points[index]);
      samples.push(last);
    }
    var hits = [];
    samples.forEach(function (sample) {
      var element = elementAt(sample[0], sample[1]);
      if (!element || element === document.documentElement || element === document.body) return;
      element = element.closest(controls) || element;
      for (var i = 0; i < hits.length; i += 1) {
        if (hits[i].element === element) {
          hits[i].count += 1;
          return;
        }
      }
      hits.push({ element: element, count: 1 });
    });
    hits.sort(function (a, b) { return b.count - a.count; });
    return hits.slice(0, 5).map(function (hit) { return describe(hit.element); });
  };
  var endStroke = function () {
    var done = stroke;
    stroke = null;
    if (done.points.length === 1) done.points.push(done.points[0].slice());
    var duration = Math.round(performance.now() - done.started);
    done.points.forEach(function (point) { point[2] = Math.min(point[2], duration); });
    fadeInk();
    post({
      kind: "stroke",
      duration: duration,
      points: done.points,
      viewport: { width: window.innerWidth, height: window.innerHeight },
      elements: underStroke(done.points),
    });
  };

  var mode = "pick";
  var drawing = null;
  // A stroke just ended: the click its release makes is the pen's, not the
  // page's, so a stroke ending on a link does not follow it.
  var penned = false;
  var swallow = function (event) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
  };
  // Clicks and the like: swallowed while picking, so links do not follow;
  // the page's own while recording, except the click ending a stroke.
  var onClick = function (event) {
    if (mode === "pen" && !stroke && !penned) return;
    if (event.type === "click") penned = false;
    swallow(event);
  };
  // While recording, Shift turns the cursor into the pen's.
  var onShift = function (event) {
    if (mode !== "pen" || event.key !== "Shift") return;
    document.documentElement.style.cursor = event.type === "keydown" ? "crosshair" : "";
  };
  var onBlur = function () {
    if (mode === "pen") document.documentElement.style.cursor = "";
  };
  var drawnRect = function (event) {
    var left = Math.min(drawing.x, event.clientX);
    var top = Math.min(drawing.y, event.clientY);
    return {
      left: left,
      top: top,
      right: Math.max(drawing.x, event.clientX),
      bottom: Math.max(drawing.y, event.clientY),
    };
  };
  var onMove = function (event) {
    if (mode === "pen") {
      if (stroke) {
        extendStroke(event);
        swallow(event);
      }
      return;
    }
    if (drawing) {
      var rect = drawnRect(event);
      box.className = "box region";
      place(box, rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top);
      box.style.display = "block";
      label.textContent = Math.round(rect.right - rect.left) + "×" + Math.round(rect.bottom - rect.top);
      place(label, Math.max(0, rect.left), rect.top >= 20 ? rect.top - 18 : rect.bottom + 2);
      label.style.display = "block";
      swallow(event);
      return;
    }
    if (mode === "region") {
      showBox(null);
      return;
    }
    showBox(elementAt(event.clientX, event.clientY));
  };
  // A region starts on a press in region mode, or on a Shift-press. Text
  // selection needs the default press, so other presses only stop reaching
  // the page's own handlers; clicks are swallowed so links do not follow.
  var onPress = function (event) {
    if (mode === "pen") {
      if (event.button !== 0 || !(event.shiftKey || stroke)) {
        penned = false;
        return;
      }
      startStroke(event);
      event.stopPropagation();
      event.stopImmediatePropagation();
      if (event.type === "mousedown") event.preventDefault();
      return;
    }
    if (event.button === 0 && (mode === "region" || event.shiftKey)) {
      drawing = drawing || { x: event.clientX, y: event.clientY };
      event.stopPropagation();
      event.stopImmediatePropagation();
      // Cancelling the pointer press would also cancel the mouse events that
      // follow it, so only the mouse press is cancelled, which is what keeps
      // the drag from selecting text.
      if (event.type === "mousedown") event.preventDefault();
      return;
    }
    event.stopPropagation();
    event.stopImmediatePropagation();
  };
  var onRelease = function (event) {
    if (event.button !== 0) return;
    if (mode === "pen") {
      if (!stroke) return;
      endStroke();
      penned = true;
      swallow(event);
      return;
    }
    if (drawing) {
      var drawn = drawnRect(event);
      drawing = null;
      swallow(event);
      showBox(null);
      var rect = visible(drawn);
      if (!rect || rect.width < 6 || rect.height < 6) return;
      var found = covering(rect);
      conceal();
      post({
        kind: "pick",
        target: {
          kind: "region",
          rect: rect,
          scroll: { x: window.scrollX, y: window.scrollY },
          viewport: { width: window.innerWidth, height: window.innerHeight },
          container: found.container,
          covers: found.covers,
          text: found.text,
        },
      });
      return;
    }
    event.stopPropagation();
    event.stopImmediatePropagation();
    var selection = window.getSelection();
    var quote = selection && !selection.isCollapsed ? String(selection).replace(/\s+/g, " ").trim() : "";
    if (quote) {
      var range = selection.getRangeAt(0);
      var node = range.commonAncestorContainer;
      var element = node.nodeType === 1 ? node : node.parentElement;
      if (element && !ownsNode(element)) {
        var shot = visible(range.getBoundingClientRect());
        conceal();
        post({
          kind: "pick",
          target: {
            kind: "selection",
            selector: selectorOf(element),
            tag: element.tagName.toLowerCase(),
            quote: quote.slice(0, 1000),
            rect: shot,
          },
        });
      }
      return;
    }
    if (mode === "region") return;
    var picked = elementAt(event.clientX, event.clientY);
    if (!picked) return;
    var pickedRect = visible(picked.getBoundingClientRect());
    showBox(null);
    conceal();
    post({
      kind: "pick",
      target: {
        kind: "element",
        selector: selectorOf(picked),
        tag: picked.tagName.toLowerCase(),
        text: textOf(picked, 400),
        html: String(picked.outerHTML || "").slice(0, 2000),
        rect: pickedRect,
      },
    });
  };
  var onKey = function (event) {
    // While recording, the page keeps its keys; Escape only drops a stroke
    // under way.
    if (mode === "pen" && !stroke) return;
    if (event.key === "Escape") {
      swallow(event);
      if (drawing) {
        drawing = null;
        showBox(null);
        return;
      }
      if (stroke) {
        stroke = null;
        inkContext.clearRect(0, 0, window.innerWidth, window.innerHeight);
        return;
      }
      post({ kind: "cancel" });
    }
  };

  var listeners = [
    ["mousemove", onMove],
    ["mousedown", onPress],
    ["pointerdown", onPress],
    ["mouseup", onRelease],
    ["click", onClick],
    ["dblclick", onClick],
    ["auxclick", onClick],
    ["submit", onClick],
    ["dragstart", onClick],
    ["keydown", onKey],
    ["keydown", onShift],
    ["keyup", onShift],
    ["blur", onBlur],
  ];
  listeners.forEach(function (entry) { window.addEventListener(entry[0], entry[1], true); });
  window.addEventListener("scroll", redraw, true);
  window.addEventListener("resize", redraw, true);
  window.addEventListener("resize", sizeInk, true);
  drawMarkers();

  window.__herdrAnnotate = {
    get popped() { return popped; },
    disarm: function () {
      listeners.forEach(function (entry) { window.removeEventListener(entry[0], entry[1], true); });
      window.removeEventListener("scroll", redraw, true);
      window.removeEventListener("resize", redraw, true);
      window.removeEventListener("resize", sizeInk, true);
      if (frame) cancelAnimationFrame(frame);
      clearTimeout(hideTimer);
      clearFade();
      if (mode !== "pick") document.documentElement.style.cursor = "";
      host.remove();
      if (window.__herdrAnnotate && window.__herdrAnnotate.host === host) delete window.__herdrAnnotate;
    },
    reveal: reveal,
    mode: function (next) {
      mode = next === "region" || next === "pen" ? next : "pick";
      drawing = null;
      stroke = null;
      penned = false;
      showBox(null);
      document.documentElement.style.cursor = mode === "region" ? "crosshair" : "";
    },
    host: host,
  };
})
