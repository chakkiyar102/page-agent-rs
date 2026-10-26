//! Console UI strings. Only the console is translated; the model's working language is `language`.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Locale {
    En,
    Zh,
}

impl Locale {
    /// "zh-CN", "zh", "zh-TW" → Zh; anything else → En.
    pub fn parse(tag: &str) -> Locale {
        if tag.to_ascii_lowercase().starts_with("zh") { Locale::Zh } else { Locale::En }
    }
}

/// Keyed lookup. `{n}` in a string is replaced by the caller.
pub fn t(l: Locale, key: &str) -> &'static str {
    let zh = l == Locale::Zh;
    match key {
        "ready" => if zh { "就绪" } else { "Ready" },
        "empty_title" => if zh { "告诉它要做什么。" } else { "Tell it what to do." },
        "empty_body" => if zh { "它把页面读成文本，然后一步步点击、输入和滚动。" } else { "It reads this page as text, then clicks, types and scrolls one step at a time." },
        "hint" => if zh { "↵ 运行 · ⇧↵ 换行" } else { "↵ run · ⇧↵ new line" },
        "task_label" => if zh { "给智能体的任务" } else { "Task for the agent" },
        "placeholder" => if zh { "帮我注册专业版…" } else { "Sign me up on the Pro plan…" },
        "stop" => if zh { "终止" } else { "Stop" },
        "run" => if zh { "运行任务" } else { "Run task" },
        "task_tag" => if zh { "任务" } else { "Task" },
        "reading" => if zh { "正在读取页面…" } else { "Reading the page…" },
        "next_action" => if zh { "下一步" } else { "Next action" },
        "step_thinking" => if zh { "第 {n} 步 · 思考中" } else { "Step {n} · thinking" },
        "step_acting" => if zh { "第 {n} 步 · 执行中" } else { "Step {n} · acting" },
        "done" => if zh { "完成" } else { "Done" },
        "not_finished" => if zh { "未完成" } else { "Didn’t finish" },
        "done_steps" => if zh { "完成 · {n} 步" } else { "Done · {n} steps" },
        "stopped" => if zh { "已终止" } else { "Stopped" },
        "collapse" => if zh { "收起控制台" } else { "Collapse console" },
        "expand" => if zh { "展开控制台" } else { "Expand console" },
        "console" => if zh { "page-agent 控制台" } else { "page-agent console" },
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_and_lookup() {
        assert_eq!(Locale::parse("zh-CN"), Locale::Zh);
        assert_eq!(Locale::parse("en-GB"), Locale::En);
        assert_eq!(t(Locale::Zh, "stop"), "终止");
        // every key used by the console exists in both locales
        for k in ["ready", "empty_title", "empty_body", "hint", "task_label", "placeholder", "stop", "run", "task_tag", "reading",
                  "next_action", "step_thinking", "step_acting", "done", "not_finished", "done_steps", "stopped", "collapse", "expand", "console"] {
            assert!(!t(Locale::En, k).is_empty() && !t(Locale::Zh, k).is_empty(), "missing {k}");
        }
    }
}
