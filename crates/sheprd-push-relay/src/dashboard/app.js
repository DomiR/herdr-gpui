// The push relay dashboard: draws the snapshots /dashboard/api/live streams
// each second (Server-Sent Events), falling back to polling
// /dashboard/api/stats when a stream cannot open. Read-only; every value
// goes in through textContent, never as HTML.
"use strict";

const POLL_MS = 5000;

const $ = (id) => document.getElementById(id);

function count(value) {
  return value.toLocaleString();
}

function duration(seconds) {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  return `${Math.floor(seconds / 86400)}d ${Math.floor((seconds % 86400) / 3600)}h`;
}

function label(text) {
  return text.charAt(0).toUpperCase() + text.slice(1).replaceAll("_", " ");
}

function setText(id, text) {
  $(id).textContent = text;
}

function sum(pairs) {
  return pairs.reduce((total, [, value]) => total + value, 0);
}

function value(pairs, name) {
  const pair = pairs.find(([key]) => key === name);
  return pair ? pair[1] : 0;
}

// A table of [name, count] pairs, with the zero rows left out unless every
// row is zero.
function table(id, pairs) {
  const shown = pairs.some(([, value]) => value > 0) ? pairs.filter(([, value]) => value > 0) : pairs;
  $(id).replaceChildren(
    ...shown.map(([name, value]) => {
      const row = document.createElement("tr");
      const key = document.createElement("td");
      key.textContent = label(name);
      const number = document.createElement("td");
      number.className = "num";
      number.textContent = count(value);
      row.append(key, number);
      return row;
    }),
  );
}

function render(stats) {
  const totals = stats.totals;
  const devices = totals.devices;
  setText("version", stats.version);
  setText("environment", `${stats.environment} APNs`);
  setText("active-day", count(devices.active_day));
  setText(
    "active-week",
    devices.skipped_pushes > 0
      ? `${count(devices.active_week)} in 7 days · ${count(devices.skipped_pushes)} pushes uncounted (full)`
      : `${count(devices.active_week)} in 7 days`,
  );
  setText("attested", count(devices.attested));
  setText(
    "untracked",
    devices.skipped_attestations > 0
      ? `${count(devices.skipped_attestations)} attestations uncounted (full)`
      : "since the relay started",
  );
  setText("delivered", count(value(totals.pushes, "delivered")));
  setText("pushes", `of ${count(sum(totals.pushes))} pushes`);
  setText("limited", count(sum(totals.limited)));
  setText("uptime", duration(totals.uptime_secs));
  table("push-replies", totals.pushes);
  table("kinds", totals.kinds);
  table("attestations", totals.attestations);
  table("challenges", totals.challenges);
  table("revocations", totals.revocations);
  table("limits", totals.limited);
}

async function poll() {
  try {
    const response = await fetch("/dashboard/api/stats", { credentials: "same-origin", cache: "no-store" });
    if (response.status === 401) {
      window.location.assign("/dashboard/login");
      return;
    }
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    render(await response.json());
    setText("status", `Updated ${new Date().toLocaleTimeString()}`);
  } catch (error) {
    setText("status", `Could not refresh: ${error.message}`);
  }
}

let polling = null;

// Polls instead, while the live stream cannot open (for example when the
// relay already serves as many streams as it allows).
function startPolling() {
  if (polling) return;
  poll();
  polling = setInterval(poll, POLL_MS);
}

function live() {
  const source = new EventSource("/dashboard/api/live");
  source.addEventListener("stats", (event) => {
    if (polling) {
      clearInterval(polling);
      polling = null;
    }
    render(JSON.parse(event.data));
    setText("status", "Live");
  });
  source.addEventListener("error", async () => {
    // EventSource reconnects on its own after a drop; a refused stream
    // (signed out, or too many open) closes it instead.
    if (source.readyState !== EventSource.CLOSED) {
      setText("status", "Reconnecting…");
      return;
    }
    try {
      const check = await fetch("/dashboard/api/stats", { credentials: "same-origin", cache: "no-store" });
      if (check.status === 401) {
        window.location.assign("/dashboard/login");
        return;
      }
      setText("status", "Live updates unavailable; refreshing every 5 s");
    } catch (error) {
      setText("status", `Could not reach the relay: ${error.message}; retrying`);
    }
    startPolling();
    setTimeout(live, 30000);
  });
}

document.addEventListener("DOMContentLoaded", live);
