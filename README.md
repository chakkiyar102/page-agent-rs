# page-agent-rs

Rust/WASM port of [alibaba/page-agent](https://github.com/alibaba/page-agent): an LLM GUI agent that runs **inside** your webpage. It reads the page as text (no screenshots), so no browser extension or headless browser is needed.

## How it works (same design as page-agent)
1. **Observe**: walk the DOM, give every visible, on-top, interactive element an index, and write it as text: `[3]<button type=submit>Create account />`. `*[i]` marks elements that are new since the last step. (`src/dom.rs`, `src/tree.rs`)
2. **Think**: one OpenAI-compatible `/chat/completions` call per step, forced to the `AgentOutput` tool: `{evaluation_previous_goal, memory, next_goal, action: {<tool>: {...}}}`. The system prompt is page-agent's, unchanged. (`src/agent.rs`)
3. **Act**: synthetic pointer and mouse events, plus the native value setter so React/Vue inputs notice the change. (`src/actions.rs`)
4. Repeat until `done` or `maxSteps` (40).

Tools: `done`, `wait`, `ask_user` (uses `window.prompt`), `click_element_by_index`, `input_text`, `select_dropdown_option`, `scroll`, `scroll_horizontally`, and `execute_javascript` (only with `experimentalScriptExecutionTool: true`).

## Demo
`demo/` is a small Workbench page: a live signup form the agent drives, a live readout of `agent.getBrowserState()` (exactly what the model sees), and a ⌘K palette of example tasks. The built `demo/pkg/` is committed, so `python3 -m http.server -d demo 8799` works straight from a clone; run `./build.sh` after changing `src/`.

Typography: Space Grotesk for display, IBM Plex Sans for body text, IBM Plex Mono for code and labels. Tokens are in `demo/tokens.css`; the console carries its own tokens in `src/panel.css`.

## Use
```sh
./build.sh && python3 -m http.server -d demo 8799   # open http://127.0.0.1:8799, set key, Start agent
```
```js
import init, { PageAgent } from "./pkg/page_agent_rs.js";
await init();
const agent = new PageAgent({ baseURL: "https://api.openai.com/v1", apiKey, model: "gpt-4.1-mini" });
agent.mountPanel();                 // optional floating console (shadow DOM, can't clash with your CSS)
agent.onEvent(e => console.log(e)); // thinking / step / executed / done / error
agent.onBeforeAction(async ({ tool, args, index, label, element }) => // optional gate, awaited before each action
  !/delete|send/i.test(label) || confirm(`Allow ${tool} on "${label}"?`)); // false/throw = skipped, model told "declined"
agent.registerTool("get_cart", {   // custom tool: description, JSON-schema parameters, execute(args)
  description: "Items in the user's cart",
  parameters: { type: "object", properties: {} },
  execute: async () => JSON.stringify(cart),   // strings go to the model as-is, other values as JSON
});
agent.registerTool("ask_user", null);  // null removes a tool; a built-in's name replaces it (`done` stays)
const { success, data } = await agent.execute("Sign me up on the Pro plan");
```
Change model or key later with `agent.setConfig({...})`. Mark parts of your page the agent should not see with `data-page-agent-ignore` (whole subtree hidden) or `data-page-agent-not-interactive` (visible, never indexed).

Config: `baseURL, apiKey, model, maxSteps, temperature, language, viewportExpansion (-1 = full page), stepDelay, instructions, customSystemPrompt, experimentalScriptExecutionTool`, plus:
- `mask` (default `true`): while a task runs, block page input and show the agent's cursor gliding to each target, with a ripple on click.
- `highlight` (default `true`): outline every indexed element with its index while a task runs.
- `locale`: console language, `"en-US"` or `"zh-CN"`; defaults to the browser's. `language` still sets the model's working language.
- `experimentalLlmsTxt`: fetch `<origin>/llms.txt` (3 s timeout, cached per origin, first 1000 chars) and add it to the prompt.

Same-origin iframes are walked like the rest of the page, so the model can fill a form inside one; cross-origin frames stay invisible (the browser won't let a page read them). The cursor, mask and highlights live in their own shadow root, below the console.

**Security:** the API key sits in the browser. Use a proxy or a scoped key for anything public, same as upstream.

## CLI
Run the agent on any URL from a terminal. It opens your installed Chrome through playwright-core, injects `demo/pkg` from the page's own origin (so CSP `'self'` allows it), runs one task and prints `{success, data}` as JSON.
```sh
cd cli && npm i && npm link                 # once
export OPENAI_API_KEY=...                     # or PAGE_AGENT_API_KEY
page-agent https://example.com "What is the page heading?"   # --headed --keep-open --model --base-url --max-steps
```
Progress events go to stderr and the result goes to stdout. Exit code 0 means success. `--headed` shows the browser with the console, cursor and highlights; `--llms-txt` turns on the llms.txt fetch. Each task handles one page: if it navigates away, the in-page agent is lost.

## Chrome extension
`extension/` is a plain MV3 extension with no build step: a popup to save your key and model and run a task on the current tab. Run `extension/build.sh`, then load the folder unpacked at `chrome://extensions`. The agent runs in the extension's isolated world, so the page can't read your key. See `extension/README.md`.

## Not ported
Upstream's multi-tab extension hub (one agent driving several tabs) and its MCP server. Use the CLI from scripts and agents instead.

`cargo test` runs the serializer, response-parser and prompt tests natively.
