# page-agent-rs Chrome extension (MV3)

1. Build: `../build.sh` (once, produces `../demo/pkg`), then `./build.sh` to copy the wasm package into `pkg/`.
2. Load: open `chrome://extensions`, turn on Developer mode, click "Load unpacked" and pick this `extension/` folder.
3. Use: click the toolbar icon, open Settings and add an API key (Base URL and model default to OpenAI `gpt-4.1-mini`), type a task and press Run task. "Open console" only mounts the in-page console, which has its own task box.

The agent runs in the extension's isolated world: `content.js` is injected on click (activeTab), dynamic-imports `pkg/page_agent_rs.js` and keeps one `PageAgent` per tab. WASM compiles there and the LLM `fetch` is not subject to the page's CSP (it is still a CORS request from the page origin, so the endpoint must allow CORS, as OpenAI does).
Settings live in `chrome.storage.local`. The key never leaves the browser except to your Base URL.
