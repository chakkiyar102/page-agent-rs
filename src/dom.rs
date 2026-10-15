//! Walks the live DOM into a `tree::Node` snapshot, indexing interactive elements.
//! Simplified port of browser-use's buildDomTree as used by page-agent.

use crate::tree::{El, Node};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

pub const PANEL_ID: &str = "page-agent-rs-panel";

const SKIP_TAGS: &[&str] = &["script", "style", "noscript", "head", "meta", "link", "template", "svg", "iframe"];
const INTERACTIVE_TAGS: &[&str] = &["a", "button", "input", "select", "textarea", "details", "summary", "option"];
const INTERACTIVE_ROLES: &[&str] = &[
    "button", "link", "checkbox", "radio", "tab", "menuitem", "menuitemcheckbox", "menuitemradio",
    "option", "switch", "combobox", "textbox", "searchbox", "slider", "spinbutton", "treeitem",
];

pub struct Snapshot {
    pub roots: Vec<Node>,
    /// index -> live element
    pub elements: Vec<Element>,
}

struct Ctx {
    doc: web_sys::Document,
    win: web_sys::Window,
    vw: f64,
    vh: f64,
    /// px beyond the viewport still considered visible; -1 = whole page
    expansion: f64,
    seen: js_sys::WeakSet,
    elements: Vec<Element>,
}

pub fn snapshot(expansion: f64, seen: &js_sys::WeakSet) -> Snapshot {
    let win = web_sys::window().unwrap();
    let doc = win.document().unwrap();
    let mut ctx = Ctx {
        vw: win.inner_width().unwrap().as_f64().unwrap_or(0.0),
        vh: win.inner_height().unwrap().as_f64().unwrap_or(0.0),
        doc: doc.clone(),
        win,
        expansion,
        seen: seen.clone(),
        elements: vec![],
    };
    let roots = match doc.body() {
        Some(b) => children_of(&b, &mut ctx, false),
        None => vec![],
    };
    Snapshot { roots, elements: ctx.elements }
}

fn children_of(el: &Element, ctx: &mut Ctx, parent_pointer: bool) -> Vec<Node> {
    let mut out = vec![];
    let mut push_list = |list: web_sys::NodeList, ctx: &mut Ctx| {
        for i in 0..list.length() {
            if let Some(n) = list.item(i) {
                if let Some(x) = build(&n, ctx, parent_pointer) {
                    out.push(x);
                }
            }
        }
    };
    if let Some(sr) = el.shadow_root() {
        push_list(sr.child_nodes(), ctx);
    }
    push_list(el.child_nodes(), ctx);
    out
}

fn build(node: &web_sys::Node, ctx: &mut Ctx, parent_pointer: bool) -> Option<Node> {
    match node.node_type() {
        web_sys::Node::TEXT_NODE => {
            let t = node.text_content()?.split_whitespace().collect::<Vec<_>>().join(" ");
            let parent = node.parent_element()?;
            (!t.is_empty() && is_visible_rect(&parent, ctx) && in_view(&parent, ctx)).then_some(Node::Text(t))
        }
        web_sys::Node::ELEMENT_NODE => {
            let el: &Element = node.unchecked_ref();
            let tag = el.tag_name().to_lowercase();
            if SKIP_TAGS.contains(&tag.as_str()) || el.id() == PANEL_ID || el.has_attribute("data-page-agent-ignore") {
                return None;
            }
            let style = ctx.win.get_computed_style(el).ok()??;
            let prop = |p: &str| style.get_property_value(p).unwrap_or_default();
            if prop("display") == "none" || prop("visibility") == "hidden" {
                return None;
            }
            let pointer = prop("cursor") == "pointer";
            let visible = is_visible_rect(el, ctx);
            let blacklisted = el.has_attribute("data-page-agent-not-interactive");
            let scroll = scroll_info(el, &prop, &tag);

            let interactive = visible
                && !blacklisted
                && in_view(el, ctx)
                && (is_interactive(el, &tag, pointer && !parent_pointer) || scroll.is_some())
                && is_top(el, ctx);

            let mut e = El { tag, scroll, ..Default::default() };
            if interactive {
                e.index = Some(ctx.elements.len());
                ctx.elements.push(el.clone());
                let obj: &js_sys::Object = el.unchecked_ref();
                if !ctx.seen.has(obj) {
                    ctx.seen.add(obj);
                    e.is_new = true;
                }
                let names = el.get_attribute_names();
                for n in names.iter().filter_map(|n| n.as_string()) {
                    if crate::tree::INCLUDE_ATTRS.contains(&n.as_str()) {
                        e.attrs.push((n.clone(), el.get_attribute(&n).unwrap_or_default()));
                    }
                }
                // live values beat stale attributes
                if let Some(i) = el.dyn_ref::<web_sys::HtmlInputElement>() {
                    set_attr(&mut e.attrs, "value", i.value());
                    if ["checkbox", "radio"].contains(&i.type_().as_str()) {
                        set_attr(&mut e.attrs, "checked", i.checked().to_string());
                    }
                } else if let Some(t) = el.dyn_ref::<web_sys::HtmlTextAreaElement>() {
                    set_attr(&mut e.attrs, "value", t.value());
                } else if let Some(s) = el.dyn_ref::<web_sys::HtmlSelectElement>() {
                    set_attr(&mut e.attrs, "value", s.value());
                }
            } else {
                e.scroll = None;
            }
            e.children = match el.dyn_ref::<web_sys::HtmlSelectElement>() {
                // options have no layout box, so list them as the select's text
                Some(s) => {
                    let o = s.options();
                    let names: Vec<String> = (0..o.length())
                        .filter_map(|i| o.item(i))
                        .filter_map(|x| x.text_content())
                        .map(|t| t.trim().to_string())
                        .collect();
                    vec![Node::Text(format!("options: {}", names.join(" | ")))]
                }
                None => children_of(el, ctx, pointer),
            };
            // prune empty non-indexed wrappers
            (e.index.is_some() || !e.children.is_empty()).then_some(Node::El(e))
        }
        _ => None,
    }
}

fn set_attr(attrs: &mut Vec<(String, String)>, k: &str, v: String) {
    attrs.retain(|(n, _)| n != k);
    attrs.push((k.into(), v));
}

fn is_interactive(el: &Element, tag: &str, own_pointer: bool) -> bool {
    if el.has_attribute("disabled") || el.get_attribute("aria-disabled").as_deref() == Some("true") {
        return false;
    }
    if tag == "input" && el.get_attribute("type").as_deref() == Some("hidden") {
        return false;
    }
    let role = el.get_attribute("role").unwrap_or_default();
    INTERACTIVE_TAGS.contains(&tag)
        || INTERACTIVE_ROLES.contains(&role.as_str())
        || el.dyn_ref::<HtmlElement>().map(|h| h.is_content_editable()).unwrap_or(false)
        || el.has_attribute("onclick")
        || el.get_attribute("tabindex").and_then(|t| t.parse::<i32>().ok()).map(|t| t >= 0).unwrap_or(false)
        || own_pointer
}

fn is_visible_rect(el: &Element, _ctx: &Ctx) -> bool {
    let r = el.get_bounding_client_rect();
    r.width() > 0.0 && r.height() > 0.0
}

fn in_view(el: &Element, ctx: &Ctx) -> bool {
    if ctx.expansion < 0.0 {
        return true;
    }
    let r = el.get_bounding_client_rect();
    let x = ctx.expansion;
    r.bottom() > -x && r.top() < ctx.vh + x && r.right() > -x && r.left() < ctx.vw + x
}

/// Is the element actually on top at its center point (not covered by a modal, etc.)?
fn is_top(el: &Element, ctx: &Ctx) -> bool {
    let r = el.get_bounding_client_rect();
    let (cx, cy) = (r.left() + r.width() / 2.0, r.top() + r.height() / 2.0);
    if cx < 0.0 || cy < 0.0 || cx > ctx.vw || cy > ctx.vh {
        return true; // ponytail: can't hit-test offscreen, assume on top
    }
    match ctx.doc.element_from_point(cx as f32, cy as f32) {
        None => true,
        Some(hit) => {
            let hn: &web_sys::Node = hit.as_ref();
            let en: &web_sys::Node = el.as_ref();
            en.contains(Some(hn)) || hn.contains(Some(en))
                // hit landed inside our shadow host's tree
                || hit.shadow_root().is_some()
        }
    }
}

fn scroll_info(el: &Element, prop: &dyn Fn(&str) -> String, tag: &str) -> Option<String> {
    if tag == "body" || tag == "html" {
        return None;
    }
    let scrolls = |v: String| ["auto", "scroll", "overlay"].contains(&v.as_str());
    let mut parts = vec![];
    if scrolls(prop("overflow-y")) && el.scroll_height() > el.client_height() {
        let top = el.scroll_top();
        let bottom = el.scroll_height() - el.client_height() - top;
        if top > 0 { parts.push(format!("top={top}")); }
        if bottom > 0 { parts.push(format!("bottom={bottom}")); }
    }
    if scrolls(prop("overflow-x")) && el.scroll_width() > el.client_width() {
        let left = el.scroll_left();
        let right = el.scroll_width() - el.client_width() - left;
        if left > 0 { parts.push(format!("left={left}")); }
        if right > 0 { parts.push(format!("right={right}")); }
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Header/footer around the element list, same wording as page-agent.
pub fn page_header_footer(expansion: f64) -> (String, String) {
    let win = web_sys::window().unwrap();
    let doc = win.document().unwrap();
    let vw = win.inner_width().unwrap().as_f64().unwrap_or(0.0);
    let vh = win.inner_height().unwrap().as_f64().unwrap_or(1.0).max(1.0);
    let de = doc.document_element().unwrap();
    let (pw, ph) = (de.scroll_width() as f64, de.scroll_height() as f64);
    let sy = win.scroll_y().unwrap_or(0.0);
    let above = sy;
    let below = (ph - vh - sy).max(0.0);
    let url = doc.location().and_then(|l| l.href().ok()).unwrap_or_default();
    let full = expansion < 0.0;
    let header = format!(
        "Current Page: [{}]({url})\nPage info: {vw}x{vh}px viewport, {pw}x{ph}px total page size, {:.1} pages above, {:.1} pages below, {:.1} total pages, at {:.0}% of page\n\n{}\n\n{}",
        doc.title(),
        above / vh,
        below / vh,
        ph / vh,
        if ph > vh { sy / (ph - vh) * 100.0 } else { 0.0 },
        if full { "Interactive elements from top layer of the current page (full page):" } else { "Interactive elements from top layer of the current page inside the viewport:" },
        if above > 4.0 && !full { format!("... {above} pixels above ({:.1} pages) - scroll to see more ...", above / vh) } else { "[Start of page]".into() },
    );
    let footer = if below > 4.0 && !full {
        format!("... {below} pixels below ({:.1} pages) - scroll to see more ...", below / vh)
    } else {
        "[End of page]".into()
    };
    (header, footer)
}
