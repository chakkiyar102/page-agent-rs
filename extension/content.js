// Injected with chrome.scripting.executeScript into the extension's isolated world.
// One PageAgent per tab: the instance lives on a global of this isolated world, so later
// injections (popup opened again) reuse it instead of loading the wasm twice.
(() => {
  if (globalThis.__pageAgentRs) return;

  const DEFAULTS = { baseURL: "https://api.openai.com/v1", apiKey: "", model: "gpt-4.1-mini" };
  let agentPromise = null;

  async function config() {
    const s = await chrome.storage.local.get(DEFAULTS);
    return { baseURL: s.baseURL || DEFAULTS.baseURL, apiKey: s.apiKey || "", model: s.model || DEFAULTS.model };
  }

  function getAgent() {
    agentPromise ??= (async () => {
      const { default: init, PageAgent } = await import(chrome.runtime.getURL("pkg/page_agent_rs.js"));
      await init({ module_or_path: chrome.runtime.getURL("pkg/page_agent_rs_bg.wasm") });
      return new PageAgent(await config());
    })();
    agentPromise.catch(() => { agentPromise = null; });
    return agentPromise;
  }

  globalThis.__pageAgentRs = {
    last: null, // promise of the most recent execute(), for whoever wants the result
    // Mount the console and, if a task is given, start it. Resolves once started (not finished),
    // so the popup can close while the agent keeps working in the page.
    async start(task) {
      const agent = await getAgent();
      agent.setConfig(await config());
      agent.mountPanel();
      if (!task) return { started: false };
      if (agent.running) throw new Error("A task is already running on this tab.");
      this.last = agent.execute(task);
      this.last.catch(() => {});
      return { started: true };
    },
    stop() { agentPromise?.then(a => a.stop()); },
  };
})();
