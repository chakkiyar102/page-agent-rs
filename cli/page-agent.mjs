#!/usr/bin/env node
// page-agent CLI: open a URL in Chrome, inject the page-agent-rs WASM build, run one task, print the result as JSON.
import { readFile } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { chromium } from "playwright-core";

const { values: opt, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    model: { type: "string", default: process.env.PAGE_AGENT_MODEL || "gpt-4.1-mini" },
    "base-url": { type: "string", default: process.env.PAGE_AGENT_BASE_URL || "https://api.openai.com/v1" },
    "max-steps": { type: "string", default: "40" },
    headed: { type: "boolean", default: false },
    "keep-open": { type: "boolean", default: false },
    help: { type: "boolean", short: "h", default: false },
  },
});
const [url, task] = positionals;
if (opt.help || !url || !task) {
  console.error(`usage: page-agent <url> "<task>" [--headed] [--keep-open] [--model m] [--base-url u] [--max-steps n]
key: PAGE_AGENT_API_KEY or OPENAI_API_KEY. Progress goes to stderr, result JSON to stdout.`);
  process.exit(opt.help ? 0 : 2);
}
const apiKey = process.env.PAGE_AGENT_API_KEY || process.env.OPENAI_API_KEY;
if (!apiKey) { console.error("page-agent: set PAGE_AGENT_API_KEY or OPENAI_API_KEY"); process.exit(2); }

const pkg = join(dirname(fileURLToPath(import.meta.url)), "..", "demo", "pkg");
const types = { js: "text/javascript", wasm: "application/wasm" };

const browser = await chromium.launch({ channel: "chrome", headless: !opt.headed });
const page = await browser.newPage();
// Serve pkg/ from the page's own origin so CSP 'self' allows the module and wasm fetch.
await page.route("**/__page_agent__/*", async (route) => {
  const file = new URL(route.request().url()).pathname.split("/").pop();
  try {
    route.fulfill({ body: await readFile(join(pkg, file)), contentType: types[file.split(".").pop()] || "application/octet-stream" });
  } catch {
    route.fulfill({ status: 404 });
  }
});
await page.exposeFunction("__pageAgentEvent", (e) => console.error(JSON.stringify(e)));
await page.goto(url, { waitUntil: "load" });

// ponytail: single page only. A task that navigates away kills the in-page agent (same as upstream);
// re-inject on framenavigated and resume if multi-page flows matter.
let result;
try {
  result = await page.evaluate(async ({ task, cfg }) => {
    const { default: init, PageAgent } = await import(location.origin + "/__page_agent__/page_agent_rs.js");
    await init();
    const agent = new PageAgent(cfg);
    agent.onEvent((e) => window.__pageAgentEvent(e));
    return await agent.execute(task);
  }, { task, cfg: { baseURL: opt["base-url"], apiKey, model: opt.model, maxSteps: Number(opt["max-steps"]) } });
} catch (err) {
  result = { success: false, data: String(err.message || err) };
}
console.log(JSON.stringify(result, null, 2));
if (!opt["keep-open"]) await browser.close();
process.exitCode = result?.success ? 0 : 1;
