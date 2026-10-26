// Browser side of the CLI: drive Chrome, inject the page-agent-rs WASM build, run tasks.
import { readFile } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const pkg = join(dirname(fileURLToPath(import.meta.url)), "..", "demo", "pkg");
const types = { js: "text/javascript", wasm: "application/wasm" };

export function configFromEnv(overrides = {}) {
  const env = process.env;
  return {
    baseURL: env.PAGE_AGENT_BASE_URL || "https://api.openai.com/v1",
    apiKey: env.PAGE_AGENT_API_KEY || env.OPENAI_API_KEY || "",
    model: env.PAGE_AGENT_MODEL || "gpt-4.1-mini",
    ...overrides,
  };
}

/** Launch installed Chrome with one page. `onEvent` gets agent events (task/step/done...). */
export async function openPage({ headed = false, storageState, onEvent = () => {} } = {}) {
  const browser = await chromium.launch({ channel: "chrome", headless: !headed });
  const page = await (await browser.newContext({ viewport: null, storageState })).newPage(); // storageState: Playwright session file for signed-in sites
  // Serve pkg/ from the page's own origin so CSP 'self' allows the module and wasm fetch.
  await page.route("**/__page_agent__/*", async (route) => {
    const file = new URL(route.request().url()).pathname.split("/").pop();
    try {
      await route.fulfill({ body: await readFile(join(pkg, file)), contentType: types[file.split(".").pop()] || "application/octet-stream" });
    } catch {
      await route.fulfill({ status: 404 });
    }
  });
  await page.exposeFunction("__pageAgentEvent", onEvent);
  return { browser, page };
}

/** Load the agent into the current document once, apply config, optionally show the console. */
async function inject(page, cfg, panel) {
  await page.evaluate(async ({ cfg, panel }) => {
    if (!window.__pageAgentRs) {
      const { default: init, PageAgent } = await import(location.origin + "/__page_agent__/page_agent_rs.js");
      await init();
      const agent = new PageAgent(cfg);
      agent.onEvent((e) => window.__pageAgentEvent(e));
      window.__pageAgentRs = agent;
    }
    window.__pageAgentRs.setConfig(cfg);
    if (panel) window.__pageAgentRs.mountPanel();
  }, { cfg, panel });
}

// ponytail: single page per task. A task that navigates away loses the in-page agent (same as upstream);
// re-inject on framenavigated and resume if multi-page flows matter.
export async function runTask(page, task, cfg, { url, panel = false } = {}) {
  if (url) {
    await page.goto(url, { waitUntil: "load" });
    await page.waitForLoadState("networkidle", { timeout: 15000 }).catch(() => {}); // let SPAs render before the agent reads the DOM
  }
  try {
    await inject(page, cfg, panel);
    return await page.evaluate((task) => window.__pageAgentRs.execute(task), task);
  } catch (err) {
    return { success: false, data: String(err.message || err) };
  }
}
