//! page-agent-rs: a Rust/WASM port of alibaba/page-agent — a GUI agent living in your webpage.
//!
//! ```js
//! import init, { PageAgent } from "./pkg/page_agent_rs.js";
//! await init();
//! const agent = new PageAgent({ baseURL: "https://api.openai.com/v1", apiKey, model: "gpt-4.1-mini" });
//! agent.mountPanel();                       // optional floating console
//! const { success, data } = await agent.execute("Fill the form with test data");
//! ```

pub mod agent;
pub mod tree;
#[cfg(target_arch = "wasm32")]
mod actions;
#[cfg(target_arch = "wasm32")]
mod dom;

#[cfg(target_arch = "wasm32")]
pub use wasm::PageAgent;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use crate::agent::{self, Config, Hist};
    use crate::{actions, dom, tree};
    use serde_json::{json, Value};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::{future_to_promise, JsFuture};
    use web_sys::Element;

    const PANEL_CSS: &str = include_str!("panel.css");
    const FONTS_ID: &str = "page-agent-rs-fonts";
    const FONTS_HREF: &str = "https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=IBM+Plex+Sans:wght@400;600&family=Space+Grotesk:wght@700&display=swap";
    const PANEL_HTML: &str = r#"<section class="pa" data-state="idle" aria-label="page-agent console">
  <header class="pa-head">
    <span class="pa-mark">page-agent<span>.rs</span></span>
    <span class="pa-status" role="status"><i class="pa-dot"></i><span data-status>Ready</span></span>
    <button class="pa-min" type="button" aria-label="Collapse console" aria-expanded="true"></button>
  </header>
  <ol class="pa-log" aria-live="polite">
    <li class="pa-empty"><strong>Tell it what to do.</strong>It reads this page as text, then clicks, types and scrolls one step at a time.<span class="pa-hint">↵ run · ⇧↵ new line</span></li>
  </ol>
  <form class="pa-form">
    <label class="pa-sr" for="pa-task">Task for the agent</label>
    <textarea class="pa-input" id="pa-task" rows="1" placeholder="Sign me up on the Pro plan…"></textarea>
    <div class="pa-actions">
      <button class="pa-btn pa-stop" type="button">Stop</button>
      <button class="pa-btn pa-run" type="submit" disabled>Run task</button>
    </div>
  </form>
</section>"#;

    struct Inner {
        cfg: RefCell<Config>,
        running: Cell<bool>,
        stop: Cell<bool>,
        history: RefCell<Vec<Hist>>,
        elements: RefCell<Vec<Element>>,
        seen: js_sys::WeakSet,
        on_event: RefCell<Option<js_sys::Function>>,
        before_action: RefCell<Option<js_sys::Function>>,
        /// the console's `.pa` root and the step row being filled in
        ui: RefCell<Option<Element>>,
        row: RefCell<Option<Element>>,
    }

    #[wasm_bindgen]
    pub struct PageAgent {
        inner: Rc<Inner>,
    }

    fn win() -> web_sys::Window {
        web_sys::window().unwrap()
    }

    fn parse_config(config: &JsValue) -> Result<Config, JsError> {
        let s = js_sys::JSON::stringify(config).map_err(|_| JsError::new("config must be an object"))?;
        serde_json::from_str(&String::from(s)).map_err(|e| JsError::new(&format!("bad config: {e}")))
    }

    #[wasm_bindgen]
    impl PageAgent {
        #[wasm_bindgen(constructor)]
        pub fn new(config: JsValue) -> Result<PageAgent, JsError> {
            Ok(PageAgent {
                inner: Rc::new(Inner {
                    cfg: RefCell::new(parse_config(&config)?),
                    running: Cell::new(false),
                    stop: Cell::new(false),
                    history: RefCell::new(vec![]),
                    elements: RefCell::new(vec![]),
                    seen: js_sys::WeakSet::new(),
                    on_event: RefCell::new(None),
                    before_action: RefCell::new(None),
                    ui: RefCell::new(None),
                    row: RefCell::new(None),
                }),
            })
        }

        /// Replace the config (e.g. after the user pastes a key). Applies to the next task.
        #[wasm_bindgen(js_name = setConfig)]
        pub fn set_config(&self, config: JsValue) -> Result<(), JsError> {
            *self.inner.cfg.borrow_mut() = parse_config(&config)?;
            Ok(())
        }

        /// `fn(event: {type, ...})` called for task/thinking/step/executed/done/error.
        #[wasm_bindgen(js_name = onEvent)]
        pub fn on_event(&self, f: js_sys::Function) {
            *self.inner.on_event.borrow_mut() = Some(f);
        }

        /// `fn({tool, args, index, label, element}) -> boolean | Promise<boolean>` awaited before every
        /// action (not `done`). `false`, a throw or a rejection skips the action and tells the model the
        /// user declined it; anything else proceeds. `element` is the target DOM node, or null.
        /// Pass `null` to remove the hook.
        #[wasm_bindgen(js_name = onBeforeAction)]
        pub fn on_before_action(&self, f: Option<js_sys::Function>) {
            *self.inner.before_action.borrow_mut() = f;
        }

        #[wasm_bindgen(getter)]
        pub fn running(&self) -> bool {
            self.inner.running.get()
        }

        /// Run a task to completion. Resolves to `{ success, data }`.
        pub fn execute(&self, task: String) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                if inner.running.replace(true) {
                    return Err(JsValue::from_str("A task is already running."));
                }
                inner.stop.set(false);
                inner.emit(json!({"type":"task","task":task}));
                let (success, data) = run(&inner, &task).await;
                inner.running.set(false);
                inner.emit(json!({"type":"done","success":success,"data":data}));
                Ok(to_js(&json!({"success":success,"data":data})))
            })
        }

        /// Abort after the current step.
        pub fn stop(&self) {
            self.inner.stop.set(true);
        }

        /// Current page as the model sees it (handy for debugging prompts).
        #[wasm_bindgen(js_name = getBrowserState)]
        pub fn get_browser_state(&self) -> String {
            let (h, c, f) = observe(&self.inner);
            format!("{h}\n{c}\n{f}")
        }

        /// Floating console, bottom-right, in a shadow root. Excluded from the agent's own view.
        #[wasm_bindgen(js_name = mountPanel)]
        pub fn mount_panel(&self) -> Result<(), JsValue> {
            let doc = win().document().unwrap();
            if doc.get_element_by_id(dom::PANEL_ID).is_some() {
                return Ok(());
            }
            if doc.get_element_by_id(FONTS_ID).is_none() {
                let link = doc.create_element("link")?;
                link.set_id(FONTS_ID);
                link.set_attribute("rel", "stylesheet")?;
                link.set_attribute("href", FONTS_HREF)?;
                doc.head().unwrap().append_child(&link)?;
            }
            let host = doc.create_element("div")?;
            host.set_id(dom::PANEL_ID);
            let shadow = host.attach_shadow(&web_sys::ShadowRootInit::new(web_sys::ShadowRootMode::Open))?;
            shadow.set_inner_html(&format!("<style>{PANEL_CSS}</style>{PANEL_HTML}"));
            doc.body().unwrap().append_child(&host)?;

            let q = |s: &str| shadow.query_selector(s).ok().flatten().unwrap();
            let root = q(".pa");
            *self.inner.ui.borrow_mut() = Some(root.clone());
            let input: web_sys::HtmlTextAreaElement = q(".pa-input").dyn_into()?;
            let run_btn: web_sys::HtmlButtonElement = q(".pa-run").dyn_into()?;
            let form = q(".pa-form");

            // Run enabled only with a task
            let (i2, b2) = (input.clone(), run_btn.clone());
            on(&input, "input", move |_| b2.set_disabled(i2.value().trim().is_empty()))?;

            // submit → run
            let (inner, i3, b3) = (self.inner.clone(), input.clone(), run_btn.clone());
            on(&form, "submit", move |e| {
                e.prevent_default();
                let task = i3.value().trim().to_string();
                if task.is_empty() || inner.running.get() {
                    return;
                }
                i3.set_value("");
                b3.set_disabled(true);
                let _ = PageAgent { inner: inner.clone() }.execute(task);
            })?;

            // Enter submits, Shift+Enter is a newline
            let f2: web_sys::HtmlFormElement = form.clone().dyn_into()?;
            on(&input, "keydown", move |e| {
                let k: &web_sys::KeyboardEvent = e.unchecked_ref();
                if k.key() == "Enter" && !k.shift_key() && !k.is_composing() {
                    e.prevent_default();
                    let _ = f2.request_submit();
                }
            })?;

            let inner = self.inner.clone();
            on(&q(".pa-stop"), "click", move |_| inner.stop.set(true))?;

            // phones: start collapsed so the console doesn't cover the page; a task expands it
            if win().inner_width().ok().and_then(|w| w.as_f64()).unwrap_or(1024.0) < 560.0 {
                root.set_attribute("data-collapsed", "")?;
                q(".pa-min").set_attribute("aria-expanded", "false")?;
            }
            let (r2, min) = (root.clone(), q(".pa-min"));
            let m2 = min.clone();
            on(&min, "click", move |_| {
                let collapsed = r2.has_attribute("data-collapsed");
                let _ = if collapsed { r2.remove_attribute("data-collapsed") } else { r2.set_attribute("data-collapsed", "") };
                let _ = m2.set_attribute("aria-expanded", if collapsed { "true" } else { "false" });
                let _ = m2.set_attribute("aria-label", if collapsed { "Collapse console" } else { "Expand console" });
            })?;
            Ok(())
        }
    }

    fn on(target: &Element, kind: &str, f: impl Fn(web_sys::Event) + 'static) -> Result<(), JsValue> {
        let cb = Closure::<dyn Fn(web_sys::Event)>::new(f);
        target.add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref())?;
        cb.forget(); // ponytail: console lives as long as the page
        Ok(())
    }

    fn to_js(v: &Value) -> JsValue {
        js_sys::JSON::parse(&v.to_string()).unwrap_or(JsValue::NULL)
    }

    /// Element with class + text. Text goes through textContent: model output is never HTML.
    fn node(tag: &str, class: &str, text: &str) -> Element {
        let e = win().document().unwrap().create_element(tag).unwrap();
        e.set_class_name(class);
        if !text.is_empty() {
            e.set_text_content(Some(text));
        }
        e
    }

    impl Inner {
        fn emit(&self, ev: Value) {
            if let Some(f) = self.on_event.borrow().as_ref() {
                let _ = f.call1(&JsValue::NULL, &to_js(&ev));
            }
            let s = |k: &str| ev[k].as_str().unwrap_or("").to_string();
            let kind = s("type");
            match kind.as_str() {
                "step" => web_sys::console::log_1(&format!("🎯 {} → {} {}", s("next_goal"), s("tool"), ev["input"]).into()),
                "executed" => web_sys::console::log_1(&format!("   {}", s("output")).into()),
                "done" => web_sys::console::log_1(&format!("🏁 {}", s("data")).into()),
                "error" => web_sys::console::warn_1(&format!("❌ {}", s("message")).into()),
                _ => {}
            }
            let Some(ui) = self.ui.borrow().clone() else { return };
            let log = ui.query_selector(".pa-log").ok().flatten().unwrap();
            let status = |t: &str| {
                if let Some(x) = ui.query_selector("[data-status]").ok().flatten() {
                    x.set_text_content(Some(t));
                }
            };
            let n = ev["step"].as_u64().unwrap_or(0) + 1;
            match kind.as_str() {
                "task" => {
                    log.set_inner_html("");
                    log.append_child(&node("li", "pa-task", &s("task"))).ok();
                    let _ = ui.set_attribute("data-state", "running");
                    ui.remove_attribute("data-collapsed").ok();
                }
                "thinking" => {
                    let row = node("li", "pa-step", "");
                    let _ = row.set_attribute("data-ok", "pending");
                    row.append_child(&node("span", "pa-n", &format!("{n:02}"))).ok();
                    let body = node("div", "", "");
                    body.append_child(&node("p", "pa-goal", "Reading the page…")).ok();
                    row.append_child(&body).ok();
                    log.append_child(&row).ok();
                    *self.row.borrow_mut() = Some(row);
                    status(&format!("Step {n} · thinking"));
                }
                "step" if s("tool") == "done" => {
                    // the final card carries the answer; a "done" step row would repeat it
                    if let Some(row) = self.row.borrow_mut().take() {
                        row.remove();
                    }
                }
                "step" => {
                    if let Some(row) = self.row.borrow().as_ref() {
                        let body = row.last_element_child().unwrap();
                        let goal = s("next_goal");
                        body.first_element_child().unwrap().set_text_content(Some(if goal.is_empty() { "Next action" } else { &goal }));
                        let call = node("code", "pa-call", "");
                        call.append_child(&node("b", "", &s("tool"))).ok();
                        let _ = call.insert_adjacent_text("beforeend", &format!("({})", agent::format_args(&ev["input"])));
                        body.append_child(&call).ok();
                        let _ = row.set_attribute("data-ok", if s("tool") == "done" { "true" } else { "pending" });
                    }
                    status(&format!("Step {n} · acting"));
                }
                "executed" => {
                    if let Some(row) = self.row.borrow().as_ref() {
                        let (ok, text) = agent::result_status(&s("output"));
                        row.last_element_child().unwrap().append_child(&node("p", "pa-out", &text)).ok();
                        let _ = row.set_attribute("data-ok", ok);
                    }
                }
                "error" => {
                    if let Some(row) = self.row.borrow().as_ref() {
                        row.last_element_child().unwrap().append_child(&node("p", "pa-out", &s("message"))).ok();
                        let _ = row.set_attribute("data-ok", "warn");
                    }
                }
                "done" => {
                    // a dangling "Reading the page…" row means we stopped mid-step
                    if let Some(row) = self.row.borrow_mut().take() {
                        if row.get_attribute("data-ok").as_deref() == Some("pending") {
                            row.remove();
                        }
                    }
                    let ok = ev["success"] == true;
                    let f = node("li", "pa-final", "");
                    let _ = f.set_attribute("data-ok", if ok { "true" } else { "false" });
                    f.append_child(&node("p", "pa-final-label", if ok { "Done" } else { "Didn’t finish" })).ok();
                    f.append_child(&node("p", "pa-final-text", &s("data"))).ok();
                    log.append_child(&f).ok();
                    let steps = self.history.borrow().iter().filter(|h| matches!(h, Hist::Step { .. })).count() + 1;
                    let _ = ui.set_attribute("data-state", if ok { "done" } else { "error" });
                    status(&if ok { format!("Done · {steps} steps") } else { "Stopped".to_string() });
                }
                _ => {}
            }
            log.set_scroll_top(log.scroll_height());
        }
    }

    /// Snapshot the page. The console is made click-through so hit-testing ignores it.
    fn observe(inner: &Inner) -> (String, String, String) {
        let panel = win().document().unwrap().get_element_by_id(dom::PANEL_ID);
        let set_pe = |v: &str| {
            if let Some(p) = panel.as_ref().and_then(|p| p.dyn_ref::<web_sys::HtmlElement>()) {
                let _ = p.style().set_property("pointer-events", v);
            }
        };
        let expansion = inner.cfg.borrow().viewport_expansion;
        set_pe("none");
        let snap = dom::snapshot(expansion, &inner.seen);
        set_pe("");
        let content = tree::serialize(&snap.roots);
        *inner.elements.borrow_mut() = snap.elements;
        let (h, f) = dom::page_header_footer(expansion);
        (h, content, f)
    }

    async fn llm(cfg: &Config, body: &Value) -> Result<Value, String> {
        let headers = web_sys::Headers::new().map_err(|e| format!("{e:?}"))?;
        let _ = headers.set("Content-Type", "application/json");
        if !cfg.api_key.is_empty() {
            let _ = headers.set("Authorization", &format!("Bearer {}", cfg.api_key));
        }
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_headers(&headers);
        init.set_body(&JsValue::from_str(&body.to_string()));
        let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
        let resp: web_sys::Response = JsFuture::from(win().fetch_with_str_and_init(&url, &init))
            .await
            .map_err(|_| format!("Network request to {url} failed"))?
            .dyn_into()
            .map_err(|_| "bad response")?;
        let text = JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
            .await
            .map_err(|e| format!("{e:?}"))?
            .as_string()
            .unwrap_or_default();
        let v: Value = serde_json::from_str(&text).map_err(|_| format!("HTTP {}: non-JSON body", resp.status()))?;
        if !resp.ok() {
            let msg = v["error"]["message"].as_str().unwrap_or("unknown error");
            return Err(match resp.status() {
                401 | 403 => format!("Authentication failed: {msg}"),
                429 => format!("Rate limit exceeded: {msg}"),
                s => format!("HTTP {s}: {msg}"),
            });
        }
        Ok(v)
    }

    async fn run(inner: &Rc<Inner>, task: &str) -> (bool, String) {
        let cfg = inner.cfg.borrow().clone();
        let cfg = &cfg;
        inner.history.borrow_mut().clear();
        let tool_defs = agent::tools(cfg);
        let known: Vec<&str> = tool_defs.iter().map(|t| t.0).collect();
        let body_base = json!({
            "model": cfg.model,
            "tools": [agent::agent_output_tool(cfg)],
            "tool_choice": {"type":"function","function":{"name":"AgentOutput"}},
            "parallel_tool_calls": false,
        });
        let system = agent::system_prompt(cfg);
        let mut last_url = String::new();
        let mut total_wait = 0.0;

        for step in 0..cfg.max_steps {
            if step > 0 {
                actions::sleep(cfg.step_delay).await;
            }
            if inner.stop.get() {
                return (false, "Stopped by you before the task finished.".into());
            }
            // observations, same triggers as page-agent
            let url = win().location().href().unwrap_or_default();
            {
                let mut h = inner.history.borrow_mut();
                if total_wait >= 3.0 {
                    h.push(Hist::Obs(format!("You have waited {total_wait} seconds accumulatively. DO NOT wait any longer unless you have a good reason.")));
                }
                if url != last_url {
                    h.push(Hist::Obs(format!("Page navigated to → {url}")));
                    last_url = url;
                }
                match cfg.max_steps - step {
                    5 => h.push(Hist::Obs("⚠️ Only 5 steps remaining. Consider wrapping up or calling done with partial results.".into())),
                    2 => h.push(Hist::Obs("⚠️ Critical: Only 2 steps left! You must finish the task or call done immediately.".into())),
                    _ => {}
                }
            }
            let (header, content, footer) = observe(inner);
            let now = String::from(js_sys::Date::new_0().to_locale_string("en-US", &JsValue::UNDEFINED));
            let user = agent::user_prompt(cfg, task, &inner.history.borrow(), &header, &content, &footer, &now);
            let mut body = body_base.clone();
            body["messages"] = json!([{"role":"system","content":system},{"role":"user","content":user}]);
            if let Some(t) = cfg.temperature {
                body["temperature"] = json!(t);
            }

            inner.emit(json!({"type":"thinking","step":step}));
            // ponytail: 3 flat retries, add backoff if rate limits bite
            let mut parsed = Err(String::new());
            for attempt in 1..=3 {
                parsed = llm(cfg, &body).await.and_then(|v| agent::parse_response(&v, &known));
                match &parsed {
                    Ok(_) => break,
                    Err(e) if e.starts_with("Authentication") => break,
                    Err(e) => inner.emit(json!({"type":"error","step":step,"message":format!("{e} (attempt {attempt} of 3)")})),
                }
            }
            let (r, tool, input) = match parsed {
                Ok(p) => p,
                Err(e) => return (false, e),
            };
            inner.emit(json!({"type":"step","step":step,"tool":tool,"input":input,"next_goal":r.next_goal,"memory":r.memory,"evaluation_previous_goal":r.evaluation_previous_goal}));

            if tool == "done" {
                let ok = input["success"].as_bool().unwrap_or(true);
                return (ok, input["text"].as_str().unwrap_or("no text provided").to_string());
            }
            let output = exec_tool(inner, cfg, &tool, &input).await;
            total_wait = if tool == "wait" { total_wait + input["seconds"].as_f64().unwrap_or(1.0) } else { 0.0 };
            inner.emit(json!({"type":"executed","step":step,"tool":tool,"output":output}));
            inner.history.borrow_mut().push(Hist::Step { r, output });
        }
        (false, "Step count exceeded maximum limit".into())
    }

    async fn exec_tool(inner: &Inner, cfg: &Config, tool: &str, input: &Value) -> String {
        let el = |i: &Value| -> Result<(Element, String), String> {
            let i = i.as_u64().ok_or("index must be a non-negative integer")? as usize;
            let e = inner.elements.borrow().get(i).cloned().ok_or(format!("No interactive element found at index {i}"))?;
            // innerText keeps layout breaks ("Team For a group"), textContent would glue them
            let raw = e.dyn_ref::<web_sys::HtmlElement>().map(|h| h.inner_text()).or_else(|| e.text_content()).unwrap_or_default();
            let label = raw.split_whitespace().collect::<Vec<_>>().join(" ");
            let label = if label.is_empty() { i.to_string() } else { label.chars().take(40).collect() };
            Ok((e, label))
        };
        let hook = inner.before_action.borrow().clone();
        if let Some(f) = hook {
            let index = input.get("index").filter(|v| !v.is_null());
            let (element, label) = match index.map(el) {
                Some(Ok((e, l))) => (JsValue::from(e), l),
                _ => (JsValue::NULL, String::new()),
            };
            let info = to_js(&json!({"tool":tool,"args":input,"index":index,"label":label}));
            let _ = js_sys::Reflect::set(&info, &"element".into(), &element);
            let allowed = match f.call1(&JsValue::NULL, &info) {
                Ok(v) => matches!(JsFuture::from(js_sys::Promise::resolve(&v)).await, Ok(v) if v != JsValue::FALSE),
                Err(_) => false,
            };
            if !allowed {
                return agent::declined(tool, &label);
            }
        }
        let vh = win().inner_height().unwrap().as_f64().unwrap_or(800.0);
        let text = input["text"].as_str().unwrap_or("");
        match tool {
            "wait" => {
                let s = input["seconds"].as_f64().unwrap_or(1.0).clamp(1.0, 10.0);
                actions::sleep(s).await;
                format!("✅ Waited for {s:.2} seconds.")
            }
            "ask_user" => {
                let q = input["question"].as_str().unwrap_or("");
                match win().prompt_with_message(q) {
                    Ok(Some(a)) => format!("User answered: {a}"),
                    _ => "User did not answer.".into(),
                }
            }
            "click_element_by_index" => match el(&input["index"]) {
                Ok((e, l)) => match actions::click(&e).await {
                    Ok(()) if e.dyn_ref::<web_sys::HtmlAnchorElement>().map(|a| a.target() == "_blank").unwrap_or(false) => {
                        format!("✅ Clicked element ({l}). ⚠️ Link opened in a new tab.")
                    }
                    Ok(()) => format!("✅ Clicked element ({l})."),
                    Err(err) => format!("❌ Failed to click element: {err}"),
                },
                Err(err) => format!("❌ Failed to click element: {err}"),
            },
            "input_text" => match el(&input["index"]) {
                Ok((e, l)) => match actions::input_text(&e, text).await {
                    Ok(()) => format!("✅ Input text ({text}) into element ({l})."),
                    Err(err) => format!("❌ Failed to input text: {err}"),
                },
                Err(err) => format!("❌ Failed to input text: {err}"),
            },
            "select_dropdown_option" => match el(&input["index"]) {
                Ok((e, l)) => match actions::select_option(&e, text).await {
                    Ok(()) => format!("✅ Selected option ({text}) in element ({l})."),
                    Err(err) => format!("❌ Failed to select option: {err}"),
                },
                Err(err) => format!("❌ Failed to select option: {err}"),
            },
            "scroll" | "scroll_horizontally" => {
                let vertical = tool == "scroll";
                let forward = input[if vertical { "down" } else { "right" }].as_bool().unwrap_or(true);
                let px = input["pixels"].as_f64().unwrap_or_else(|| input["num_pages"].as_f64().unwrap_or(0.1) * vh);
                let px = if forward { px } else { -px };
                let target = match input.get("index").filter(|v| !v.is_null()) {
                    Some(i) => match el(i) {
                        Ok((e, _)) => Some(e),
                        Err(err) => return format!("❌ Failed to scroll: {err}"),
                    },
                    None => None,
                };
                actions::scroll(target.as_ref(), px, vertical).await
            }
            "execute_javascript" if cfg.experimental_script_execution_tool => {
                actions::execute_js(input["script"].as_str().unwrap_or("")).await
            }
            other => format!("❌ Tool {other} not found"),
        }
    }
}
