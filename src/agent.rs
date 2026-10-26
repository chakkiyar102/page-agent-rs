//! The agent loop: observe -> one forced `AgentOutput` tool call -> execute -> repeat.

use serde::Deserialize;
use serde_json::{json, Value};

const SYSTEM_PROMPT: &str = include_str!("system_prompt.txt");

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(rename = "baseURL")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub model: String,
    #[serde(default = "d_max_steps")]
    pub max_steps: usize,
    pub temperature: Option<f64>,
    /// e.g. "中文"; replaces the default working language
    pub language: Option<String>,
    /// px around the viewport treated as visible; -1 = whole page
    #[serde(default)]
    pub viewport_expansion: f64,
    #[serde(default = "d_step_delay")]
    pub step_delay: f64,
    #[serde(default)]
    pub experimental_script_execution_tool: bool,
    /// Fetch `<origin>/llms.txt` and give it to the model (first 1000 chars).
    #[serde(default)]
    pub experimental_llms_txt: bool,
    /// Block user input and show the agent cursor while a task runs.
    #[serde(default = "d_true")]
    pub mask: bool,
    /// Outline indexed elements while a task runs.
    #[serde(default = "d_true")]
    pub highlight: bool,
    /// Console language: "en-US" | "zh-CN". Default: the browser's language.
    pub locale: Option<String>,
    /// Extra system-level instructions for this site.
    pub instructions: Option<String>,
    pub custom_system_prompt: Option<String>,
}
fn d_max_steps() -> usize { 40 }
fn d_step_delay() -> f64 { 0.4 }
fn d_true() -> bool { true }

pub struct Reflection {
    pub evaluation_previous_goal: String,
    pub memory: String,
    pub next_goal: String,
}

pub enum Hist {
    Step { r: Reflection, output: String },
    Obs(String),
}

/// (name, description, JSON schema of the input).
pub type ToolDef = (String, String, Value);

/// Built-in tools, in page-agent's order.
pub fn tools(cfg: &Config) -> Vec<ToolDef> {
    let idx = json!({"type": "integer", "minimum": 0});
    let mut t = vec![
        ("done", "Complete task. Text is your final response to the user — keep it concise unless the user explicitly asks for detail.",
            json!({"type":"object","properties":{"text":{"type":"string"},"success":{"type":"boolean","default":true}},"required":["text"]})),
        ("wait", "Wait for x seconds. Can be used to wait until the page or data is fully loaded.",
            json!({"type":"object","properties":{"seconds":{"type":"number","minimum":1,"maximum":10,"default":1}}})),
        ("ask_user", "Ask the user a question and wait for their answer. Use this if you need more information or clarification.",
            json!({"type":"object","properties":{"question":{"type":"string"}},"required":["question"]})),
        ("click_element_by_index", "Click element by index",
            json!({"type":"object","properties":{"index":idx},"required":["index"]})),
        ("input_text", "Click and type text into an interactive input element",
            json!({"type":"object","properties":{"index":idx,"text":{"type":"string"}},"required":["index","text"]})),
        ("select_dropdown_option", "Select dropdown option for interactive element index by the text of the option you want to select",
            json!({"type":"object","properties":{"index":idx,"text":{"type":"string"}},"required":["index","text"]})),
        ("scroll", "Scroll vertically. Without index: scrolls the document. With index: scrolls the container at that index (or its nearest scrollable ancestor). Use index of a data-scrollable element to scroll a specific area.",
            json!({"type":"object","properties":{"down":{"type":"boolean","default":true},"num_pages":{"type":"number","minimum":0,"maximum":10,"default":0.1},"pixels":idx,"index":idx}})),
        ("scroll_horizontally", "Scroll horizontally. Without index: scrolls the document. With index: scrolls the container at that index (or its nearest scrollable ancestor). Use index of a data-scrollable element to scroll a specific area.",
            json!({"type":"object","properties":{"right":{"type":"boolean","default":true},"pixels":idx,"index":idx},"required":["pixels"]})),
    ];
    if cfg.experimental_script_execution_tool {
        t.push(("execute_javascript", "Execute JavaScript code on the current page. Supports async/await syntax. Use with caution!",
            json!({"type":"object","properties":{"script":{"type":"string"}},"required":["script"]})));
    }
    t.into_iter().map(|(n, d, s)| (n.to_string(), d.to_string(), s)).collect()
}

/// Built-ins minus `removed`, with `custom` replacing same-named built-ins or appended. `done` always stays.
pub fn merge_tools(builtin: Vec<ToolDef>, custom: &[ToolDef], removed: &[String]) -> Vec<ToolDef> {
    let mut t: Vec<ToolDef> = builtin.into_iter().filter(|(n, _, _)| n == "done" || !removed.contains(n)).collect();
    for c in custom {
        match t.iter_mut().find(|(n, _, _)| *n == c.0) {
            Some(slot) => *slot = c.clone(),
            None => t.push(c.clone()),
        }
    }
    t
}

/// The single macro tool the model must call each step: reflection + exactly one action.
pub fn agent_output_tool(tools: &[ToolDef]) -> Value {
    let variants: Vec<Value> = tools
        .iter()
        .map(|(name, desc, schema)| json!({"type":"object","description":desc,"properties":{name: schema},"required":[name],"additionalProperties":false}))
        .collect();
    json!({"type":"function","function":{
        "name":"AgentOutput",
        "description":"You MUST call this tool every step!",
        "parameters":{"type":"object","properties":{
            "evaluation_previous_goal":{"type":"string"},
            "memory":{"type":"string"},
            "next_goal":{"type":"string"},
            "action":{"anyOf":variants}
        },"required":["action"]}
    }})
}

pub fn system_prompt(cfg: &Config) -> String {
    if let Some(p) = &cfg.custom_system_prompt {
        return p.clone();
    }
    match &cfg.language {
        Some(l) => SYSTEM_PROMPT.replace("Default working language: **English**", &format!("Default working language: **{l}**")),
        None => SYSTEM_PROMPT.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn user_prompt(cfg: &Config, task: &str, history: &[Hist], header: &str, content: &str, footer: &str, now: &str, llms_txt: Option<&str>) -> String {
    let mut t = String::new();
    let sys = cfg.instructions.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let llms = llms_txt.map(str::trim).filter(|s| !s.is_empty());
    if sys.is_some() || llms.is_some() {
        t += "<instructions>\n";
        if let Some(i) = sys {
            t += &format!("<system_instructions>\n{i}\n</system_instructions>\n");
        }
        if let Some(l) = llms {
            t += &format!("<llms_txt>\n{l}\n</llms_txt>\n");
        }
        t += "</instructions>\n\n";
    }
    let steps = history.iter().filter(|h| matches!(h, Hist::Step { .. })).count();
    t += &format!(
        "<agent_state>\n<user_request>\n{task}\n</user_request>\n<step_info>\nStep {} of {} max possible steps\nCurrent time: {now}\n</step_info>\n</agent_state>\n\n<agent_history>\n",
        steps + 1,
        cfg.max_steps
    );
    let mut n = 0;
    for h in history {
        match h {
            Hist::Step { r, output } => {
                n += 1;
                t += &format!(
                    "<step_{n}>\nEvaluation of Previous Step: {}\nMemory: {}\nNext Goal: {}\nAction Results: {output}\n</step_{n}>\n",
                    r.evaluation_previous_goal, r.memory, r.next_goal
                );
            }
            Hist::Obs(o) => t += &format!("<sys>{o}</sys>\n"),
        }
    }
    t += &format!("</agent_history>\n\n<browser_state>\n{header}\n{content}\n{footer}\n\n</browser_state>\n\n");
    t
}

/// Pull `(reflection, tool, input)` out of a chat-completions response, forgiving the
/// usual model quirks (calling the inner tool directly, stringified `action`, flat args).
pub fn parse_response(resp: &Value, known: &[&str]) -> Result<(Reflection, String, Value), String> {
    let choice = resp["choices"].get(0).ok_or("No choices in response")?;
    match choice["finish_reason"].as_str() {
        Some("length") => return Err("Response truncated: max tokens reached".into()),
        Some("content_filter") => return Err("Content filtered by safety system".into()),
        _ => {}
    }
    let call = &choice["message"]["tool_calls"][0]["function"];
    let name = call["name"].as_str().ok_or("No tool call found in response")?;
    let args_raw = &call["arguments"];
    let mut args: Value = match args_raw {
        Value::String(s) => serde_json::from_str(s).map_err(|e| format!("Invalid tool arguments JSON: {e}"))?,
        v => v.clone(),
    };
    if name != "AgentOutput" {
        if !known.contains(&name) {
            return Err(format!("Tool \"{name}\" not found in tools"));
        }
        args = json!({ "action": { name: args } });
    }
    if let Some(s) = args["action"].as_str() {
        args["action"] = serde_json::from_str(s).map_err(|e| format!("Invalid action JSON: {e}"))?;
    }
    if args.get("action").is_none() {
        if let Some(k) = known.iter().find(|k| args.get(**k).is_some()) {
            let inner = args[*k].take();
            args["action"] = json!({ *k: inner });
        }
    }
    let action = args["action"].as_object().ok_or("Missing action")?;
    let (tool, input) = action.iter().next().ok_or("Empty action")?;
    if !known.contains(&tool.as_str()) {
        return Err(format!("Tool \"{tool}\" not found in tools"));
    }
    let s = |k: &str| args[k].as_str().unwrap_or("").to_string();
    Ok((
        Reflection { evaluation_previous_goal: s("evaluation_previous_goal"), memory: s("memory"), next_goal: s("next_goal") },
        tool.clone(),
        input.clone(),
    ))
}

/// `input_text {index:0,text:"Ada"}` → `index: 0, text: "Ada"` for the console.
pub fn format_args(input: &Value) -> String {
    match input.as_object() {
        Some(o) => o.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(", "),
        None => String::new(),
    }
}

/// Split a tool result into (status, text): "true" | "false" | "warn", emoji prefix removed.
pub fn result_status(output: &str) -> (&'static str, String) {
    let status = if output.starts_with('❌') {
        "false"
    } else if output.contains('⚠') {
        "warn"
    } else {
        "true"
    };
    let text = output.trim_start_matches(|c: char| !c.is_alphanumeric() && c != '(').trim().to_string();
    (status, text)
}

/// Tool result when the `onBeforeAction` hook said no. Starts with ❌ so the console shows it as not done.
pub fn declined(tool: &str, label: &str) -> String {
    let on = if label.is_empty() { String::new() } else { format!(" on element ({label})") };
    format!("❌ The user declined this action ({tool}{on}). It was not performed. Do not retry it; continue another way or call done and explain.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_formatting() {
        assert_eq!(format_args(&json!({"index":0,"text":"Ada"})), "index: 0, text: \"Ada\"");
        assert_eq!(result_status("✅ Clicked element (Pro)."), ("true", "Clicked element (Pro).".into()));
        assert_eq!(result_status("❌ Failed to click element: x").0, "false");
        assert_eq!(result_status("⚠️ Already at the bottom").0, "warn");
        assert_eq!(result_status(&declined("click_element_by_index", "Send")).0, "false");
        assert!(declined("click_element_by_index", "Send").contains("declined this action (click_element_by_index on element (Send))"));
        assert!(declined("scroll", "").contains("(scroll)"));
    }

    fn resp(name: &str, args: &str) -> Value {
        json!({"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[{"function":{"name":name,"arguments":args}}]}}]})
    }
    const K: &[&str] = &["done", "click_element_by_index"];

    #[test]
    fn parses_macro_tool() {
        let (r, t, i) = parse_response(&resp("AgentOutput", r#"{"memory":"m","action":{"click_element_by_index":{"index":3}}}"#), K).unwrap();
        assert_eq!((r.memory.as_str(), t.as_str(), i["index"].as_u64()), ("m", "click_element_by_index", Some(3)));
    }

    #[test]
    fn normalizes_quirks() {
        let (_, t, _) = parse_response(&resp("done", r#"{"text":"ok"}"#), K).unwrap();
        assert_eq!(t, "done");
        let (_, t, i) = parse_response(&resp("AgentOutput", r#"{"action":"{\"done\":{\"text\":\"x\"}}"}"#), K).unwrap();
        assert_eq!((t.as_str(), i["text"].as_str()), ("done", Some("x")));
        let (_, t, _) = parse_response(&resp("AgentOutput", r#"{"done":{"text":"x"}}"#), K).unwrap();
        assert_eq!(t, "done");
        assert!(parse_response(&resp("AgentOutput", r#"{"action":{"nope":{}}}"#), K).is_err());
    }

    #[test]
    fn prompt_has_history_and_state() {
        let cfg: Config = serde_json::from_value(json!({"baseURL":"x","model":"m"})).unwrap();
        let h = vec![Hist::Step { r: Reflection { evaluation_previous_goal: "e".into(), memory: "m".into(), next_goal: "n".into() }, output: "✅ ok".into() }, Hist::Obs("nav".into())];
        let p = user_prompt(&cfg, "do it", &h, "H", "C", "F", "now", None);
        assert!(p.contains("Step 2 of 40 max possible steps"));
        assert!(p.contains("<step_1>\nEvaluation of Previous Step: e\nMemory: m\nNext Goal: n\nAction Results: ✅ ok\n</step_1>"));
        assert!(p.contains("<sys>nav</sys>") && p.contains("<browser_state>\nH\nC\nF"));
        assert!(system_prompt(&cfg).contains("<user_request>") && agent_output_tool(&tools(&cfg))["function"]["name"] == "AgentOutput");
        let p = user_prompt(&cfg, "t", &[], "H", "C", "F", "now", Some("# Site\nUse /docs"));
        assert!(p.starts_with("<instructions>\n<llms_txt>\n# Site\nUse /docs\n</llms_txt>\n</instructions>"));
    }

    #[test]
    fn custom_tools_merge() {
        let cfg: Config = serde_json::from_value(json!({"baseURL":"x","model":"m"})).unwrap();
        let mine = ("ask_user".to_string(), "mine".to_string(), json!({}));
        let extra = ("get_cart".to_string(), "cart".to_string(), json!({}));
        let t = merge_tools(tools(&cfg), &[mine, extra], &["scroll".into(), "done".into()]);
        let names: Vec<&str> = t.iter().map(|x| x.0.as_str()).collect();
        assert!(names.contains(&"done") && !names.contains(&"scroll") && names.last() == Some(&"get_cart"));
        assert_eq!(t.iter().find(|x| x.0 == "ask_user").unwrap().1, "mine");
        let cfg: Config = serde_json::from_value(json!({"baseURL":"x","model":"m"})).unwrap();
        assert!(cfg.mask && cfg.highlight && !cfg.experimental_llms_txt);
    }
}
