//! Pure, DOM-free model of the page snapshot + the text serializer the LLM reads.
//! Mirrors page-agent's `flatTreeToString`.

pub enum Node {
    Text(String),
    El(El),
}

#[derive(Default)]
pub struct El {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub index: Option<usize>,
    pub is_new: bool,
    /// "top=.., bottom=.." remaining scroll distances, if the element scrolls.
    pub scroll: Option<String>,
    pub children: Vec<Node>,
}

/// Attributes surfaced to the model, in page-agent's order.
pub const INCLUDE_ATTRS: &[&str] = &[
    "title", "type", "checked", "name", "role", "value", "placeholder", "data-date-format",
    "alt", "aria-label", "aria-expanded", "data-state", "aria-checked", "id", "for", "target",
    "aria-haspopup", "aria-controls", "aria-owns", "contenteditable",
];

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}...", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Text of an element up to (not into) the next indexed descendant.
// ponytail: joined with spaces, not "\n", so one element stays on one line.
fn text_till_next_clickable(e: &El) -> String {
    fn go(n: &Node, out: &mut Vec<String>) {
        match n {
            Node::Text(t) => out.push(t.clone()),
            Node::El(e) if e.index.is_none() => e.children.iter().for_each(|c| go(c, out)),
            Node::El(_) => {}
        }
    }
    let mut out = vec![];
    e.children.iter().for_each(|c| go(c, &mut out));
    out.join(" ").trim().to_string()
}

fn attr_string(e: &El, text: &str) -> String {
    let mut picked: Vec<(String, String)> = INCLUDE_ATTRS
        .iter()
        .filter_map(|k| e.attrs.iter().find(|(n, _)| n == k))
        .map(|(k, v)| (k.clone(), v.trim().to_string()))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    // drop long duplicate values (keep first occurrence)
    if picked.len() > 1 {
        let mut seen: Vec<String> = vec![];
        picked.retain(|(_, v)| {
            if v.len() <= 5 {
                return true;
            }
            if seen.contains(v) {
                return false;
            }
            seen.push(v.clone());
            true
        });
    }
    let lt = text.trim().to_lowercase();
    picked.retain(|(k, v)| {
        !(k == "role" && *v == e.tag)
            && !(["aria-label", "placeholder", "title"].contains(&k.as_str()) && v.trim().to_lowercase() == lt)
    });
    picked.iter().map(|(k, v)| format!("{k}={}", trunc(v, 20))).collect::<Vec<_>>().join(" ")
}

pub fn serialize(roots: &[Node]) -> String {
    fn walk(n: &Node, depth: usize, under_indexed: bool, out: &mut Vec<String>) {
        let tabs = "\t".repeat(depth);
        match n {
            Node::Text(t) => {
                if !under_indexed {
                    out.push(format!("{tabs}{t}"));
                }
            }
            Node::El(e) => {
                let mut d = depth;
                if let Some(i) = e.index {
                    d += 1;
                    let text = text_till_next_clickable(e);
                    let attrs = attr_string(e, &text);
                    let mut line = format!("{tabs}{}[{i}]<{}", if e.is_new { "*" } else { "" }, e.tag);
                    if !attrs.is_empty() {
                        line += &format!(" {attrs}");
                    }
                    if let Some(s) = &e.scroll {
                        line += &format!(" data-scrollable=\"{s}\"");
                    }
                    if attrs.is_empty() {
                        line.push(' ');
                    }
                    if !text.is_empty() {
                        line += &format!(">{text}");
                    }
                    line += " />";
                    out.push(line);
                }
                for c in &e.children {
                    walk(c, d, under_indexed || e.index.is_some(), out);
                }
            }
        }
    }
    let mut out = vec![];
    roots.iter().for_each(|n| walk(n, 0, false, &mut out));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(tag: &str, index: Option<usize>, attrs: &[(&str, &str)], children: Vec<Node>) -> Node {
        Node::El(El {
            tag: tag.into(),
            index,
            attrs: attrs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            children,
            ..Default::default()
        })
    }

    #[test]
    fn serializes_like_page_agent() {
        let tree = vec![el("div", None, &[], vec![
            Node::Text("Login".into()),
            el("form", Some(0), &[("id", "f")], vec![
                Node::Text("Sign in".into()),
                el("input", Some(1), &[("type", "text"), ("placeholder", "Name")], vec![]),
                el("button", Some(2), &[("role", "button"), ("aria-label", "Go")], vec![Node::Text("Go".into())]),
            ]),
        ])];
        let s = serialize(&tree);
        assert_eq!(
            s,
            "Login\n[0]<form id=f>Sign in />\n\t[1]<input type=text placeholder=Name />\n\t[2]<button >Go />"
        );
    }

    #[test]
    fn truncates_and_marks_new() {
        let mut e = El { tag: "a".into(), index: Some(7), is_new: true, ..Default::default() };
        e.attrs.push(("title".into(), "a very long title that goes on".into()));
        e.scroll = Some("bottom=40".into());
        assert_eq!(serialize(&[Node::El(e)]), "*[7]<a title=a very long title th... data-scrollable=\"bottom=40\" />");
    }
}
