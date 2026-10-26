#!/usr/bin/env node
// page-agent CLI: open a URL in Chrome, inject the page-agent-rs WASM build, run one task, print the result as JSON.
import { parseArgs } from "node:util";
import { configFromEnv, openPage, runTask } from "./lib.mjs";

const { values: opt, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    model: { type: "string" },
    "base-url": { type: "string" },
    "max-steps": { type: "string", default: "40" },
    "storage-state": { type: "string" },
    headed: { type: "boolean", default: false },
    "keep-open": { type: "boolean", default: false },
    "llms-txt": { type: "boolean", default: false },
    help: { type: "boolean", short: "h", default: false },
  },
});
const [url, task] = positionals;
if (opt.help || !url || !task) {
  console.error(`usage: page-agent <url> "<task>" [--headed] [--keep-open] [--llms-txt] [--model m] [--base-url u] [--max-steps n] [--storage-state file.json]
key: PAGE_AGENT_API_KEY or OPENAI_API_KEY. Progress goes to stderr, result JSON to stdout.
--headed shows the browser with the console, cursor and highlights.
--storage-state loads a Playwright session file (cookies + localStorage) for sites that need a sign-in.`);
  process.exit(opt.help ? 0 : 2);
}
const cfg = configFromEnv({
  maxSteps: Number(opt["max-steps"]),
  experimentalLlmsTxt: opt["llms-txt"],
  ...(opt.model && { model: opt.model }),
  ...(opt["base-url"] && { baseURL: opt["base-url"] }),
});
if (!cfg.apiKey) { console.error("page-agent: set PAGE_AGENT_API_KEY or OPENAI_API_KEY"); process.exit(2); }

const { browser, page } = await openPage({ headed: opt.headed, storageState: opt["storage-state"], onEvent: (e) => console.error(JSON.stringify(e)) });
const result = await runTask(page, task, cfg, { url, panel: opt.headed });
console.log(JSON.stringify(result, null, 2));
if (!opt["keep-open"]) await browser.close();
process.exitCode = result?.success ? 0 : 1;
