const DEFAULTS = { baseURL: "https://api.openai.com/v1", apiKey: "", model: "gpt-4.1-mini" };
const $ = (id) => document.getElementById(id);

function say(text, kind = "") {
  $("status").textContent = text;
  $("status").dataset.kind = kind;
}

async function loadSettings() {
  const s = await chrome.storage.local.get(DEFAULTS);
  $("baseURL").value = s.baseURL;
  $("apiKey").value = s.apiKey;
  $("model").value = s.model;
  if (!s.apiKey) $("settings").open = true;
  return s;
}

async function saveSettings() {
  await chrome.storage.local.set({
    baseURL: $("baseURL").value.trim() || DEFAULTS.baseURL,
    apiKey: $("apiKey").value.trim(),
    model: $("model").value.trim() || DEFAULTS.model,
  });
}

// Inject content.js (isolated world) into the tab, then mount the console and optionally start a task.
export async function inject(tabId, task) {
  await chrome.scripting.executeScript({ target: { tabId }, files: ["content.js"] });
  const [res] = await chrome.scripting.executeScript({
    target: { tabId },
    func: (t) => globalThis.__pageAgentRs.start(t).catch((e) => ({ error: String(e?.message ?? e) })),
    args: [task || ""],
  });
  if (res?.result?.error) throw new Error(res.result.error);
  return res?.result;
}

async function activeTabId() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id || !/^https?:|^file:/.test(tab.url ?? "https:")) {
    throw new Error("This page cannot be scripted. Open a regular web page.");
  }
  return tab.id;
}

async function go(task) {
  const buttons = document.querySelectorAll("button");
  buttons.forEach((b) => (b.disabled = true));
  try {
    if (task) {
      const { apiKey } = await chrome.storage.local.get(DEFAULTS);
      if (!apiKey) {
        $("settings").open = true;
        $("apiKey").focus();
        throw new Error("Add an API key in Settings first.");
      }
    }
    say(task ? "Starting..." : "Opening console...");
    await inject(await activeTabId(), task);
    say(task ? "Running. Follow along in the page console." : "Console is open on the page.", "ok");
  } catch (e) {
    say(String(e.message ?? e), "error");
  } finally {
    buttons.forEach((b) => (b.disabled = false));
  }
}

$("task-form").addEventListener("submit", (e) => {
  e.preventDefault();
  const task = $("task").value.trim();
  if (!task) { say("Type a task first.", "error"); $("task").focus(); return; }
  go(task);
});
$("task").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) $("task-form").requestSubmit();
});
$("console").addEventListener("click", () => go(""));
$("settings-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  await saveSettings();
  say("Settings saved.", "ok");
});

loadSettings().then(() => $("task").focus());
