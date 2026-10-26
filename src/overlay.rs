//! Visual layer while a task runs: input-blocking mask, the agent's cursor, and outlines on
//! indexed elements. Lives in its own shadow root, below the console, ignored by the snapshot.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, ShadowRoot};

pub const OVERLAY_ID: &str = "page-agent-rs-overlay";

const CSS: &str = r#"
:host {
  --ov-accent: oklch(58% 0.2 256);
  --ov-accent-ink: oklch(99% 0.003 250);
  --ov-outline: oklch(98.5% 0.004 250);
  --ov-scrim: oklch(24% 0.02 258 / 0.05);
  --ov-ease: cubic-bezier(0.16, 1, 0.3, 1);
  position: fixed; inset: 0; z-index: 2147482990;
  pointer-events: none;
}
:host([data-mask]) { pointer-events: auto; cursor: progress; }
:host([data-pass]) { pointer-events: none; }
.frame {
  position: absolute; inset: 0; opacity: 0;
  background: var(--ov-scrim);
  box-shadow: inset 0 0 0 2px oklch(58% 0.2 256 / 0.55);
  transition: opacity 240ms var(--ov-ease);
}
:host([data-on]) .frame { opacity: 1; }
:host(:not([data-mask])) .frame { background: none; }

.hl {
  position: absolute;
  border: 1.5px solid oklch(58% 0.2 256 / 0.7);
  border-radius: 3px;
  background: oklch(58% 0.2 256 / 0.05);
}
.hl[data-active] {
  border: 2px solid var(--ov-accent);
  background: oklch(58% 0.2 256 / 0.12);
}
.hl i {
  position: absolute; left: -1.5px; bottom: 100%;
  padding: 1px 4px;
  font: 500 10px/1.3 "IBM Plex Mono", ui-monospace, Menlo, monospace;
  font-style: normal;
  color: var(--ov-accent-ink);
  background: var(--ov-accent);
  border-radius: 3px 3px 0 0;
  white-space: nowrap;
}
.hl[data-flip] i { bottom: auto; top: 0; border-radius: 0 0 3px 0; }

/* the agent's pointer: an arrow drawn with clip-path, outlined by a larger copy behind it */
.cursor {
  position: absolute; left: 0; top: 0; width: 20px; height: 24px;
  opacity: 0;
  transform: translate(var(--x, 50vw), var(--y, 50vh));
  transition: transform 420ms var(--ov-ease), opacity 200ms var(--ov-ease);
  filter: drop-shadow(0 2px 3px oklch(24% 0.02 258 / 0.35));
}
:host([data-on]) .cursor { opacity: 1; }
.cursor::before, .cursor::after {
  content: ""; position: absolute; inset: 0;
  clip-path: polygon(0 0, 0 82%, 26% 64%, 44% 100%, 58% 94%, 41% 59%, 76% 59%);
}
.cursor::before { background: var(--ov-outline); transform: scale(1.18); transform-origin: 0 0; translate: -1.5px -2px; }
.cursor::after { background: var(--ov-accent); }
.ripple {
  position: absolute; left: -14px; top: -14px; width: 28px; height: 28px;
  border: 2px solid var(--ov-accent); border-radius: 50%;
  opacity: 0;
}
.cursor[data-click] .ripple { animation: ripple 360ms var(--ov-ease); }
@keyframes ripple { from { transform: scale(0.2); opacity: 1; } to { transform: scale(1.6); opacity: 0; } }

@media (prefers-reduced-motion: reduce) {
  .cursor, .frame { transition: none; }
  .cursor[data-click] .ripple { animation: none; }
}
"#;

fn doc() -> web_sys::Document {
    web_sys::window().unwrap().document().unwrap()
}

fn host() -> Option<Element> {
    doc().get_element_by_id(OVERLAY_ID)
}

fn root() -> Option<ShadowRoot> {
    host().and_then(|h| h.shadow_root())
}

fn q(sel: &str) -> Option<Element> {
    root().and_then(|r| r.query_selector(sel).ok().flatten())
}

/// Create the overlay once. Pointer events on it are swallowed so the user can't fight the agent.
fn ensure() -> Result<(), JsValue> {
    if host().is_some() {
        return Ok(());
    }
    let d = doc();
    let h = d.create_element("div")?;
    h.set_id(OVERLAY_ID);
    h.set_attribute("aria-hidden", "true")?;
    let sr = h.attach_shadow(&web_sys::ShadowRootInit::new(web_sys::ShadowRootMode::Open))?;
    sr.set_inner_html(&format!(r#"<style>{CSS}</style><div class="frame"></div><div class="boxes"></div><div class="cursor"><div class="ripple"></div></div>"#));
    let swallow = Closure::<dyn Fn(web_sys::Event)>::new(|e: web_sys::Event| {
        e.prevent_default();
        e.stop_propagation();
    });
    for kind in ["click", "mousedown", "mouseup", "pointerdown", "pointerup", "wheel", "touchstart", "touchmove", "contextmenu"] {
        h.add_event_listener_with_callback(kind, swallow.as_ref().unchecked_ref())?;
    }
    swallow.forget(); // ponytail: overlay lives as long as the page
    d.body().unwrap().append_child(&h)?;
    Ok(())
}

pub fn show(mask: bool) {
    if ensure().is_err() {
        return;
    }
    let h = host().unwrap();
    let _ = h.set_attribute("data-on", "");
    let _ = if mask { h.set_attribute("data-mask", "") } else { h.remove_attribute("data-mask") };
}

pub fn hide() {
    if let Some(h) = host() {
        let _ = h.remove_attribute("data-on");
        let _ = h.remove_attribute("data-mask");
    }
    clear_boxes();
}

/// Let hit-testing (snapshot, synthetic clicks) see through the mask for a moment.
pub fn passthrough(on: bool) {
    if let Some(h) = host() {
        let _ = if on { h.set_attribute("data-pass", "") } else { h.remove_attribute("data-pass") };
    }
}

/// Top-level viewport offset of an element living in (nested) same-origin iframes.
pub fn frame_offset(el: &Element) -> (f64, f64) {
    let (mut x, mut y) = (0.0, 0.0);
    let mut view = el.owner_document().and_then(|d| d.default_view());
    while let Some(fe) = view.as_ref().and_then(|w| w.frame_element().ok().flatten()) {
        let r = fe.get_bounding_client_rect();
        x += r.left() + fe.client_left() as f64;
        y += r.top() + fe.client_top() as f64;
        view = fe.owner_document().and_then(|d| d.default_view());
    }
    (x, y)
}

pub fn clear_boxes() {
    if let Some(b) = q(".boxes") {
        b.set_inner_html("");
    }
}

/// Outline every indexed element with its index chip.
pub fn draw_boxes(elements: &[Element]) {
    clear_boxes();
    let Some(boxes) = q(".boxes") else { return };
    let d = doc();
    for (i, el) in elements.iter().enumerate() {
        let r = el.get_bounding_client_rect();
        if r.width() <= 0.0 || r.height() <= 0.0 {
            continue;
        }
        let (ox, oy) = frame_offset(el);
        let Ok(b) = d.create_element("div") else { continue };
        b.set_class_name("hl");
        let (top, left) = (r.top() + oy, r.left() + ox);
        let _ = b.set_attribute("style", &format!("left:{left}px;top:{top}px;width:{}px;height:{}px", r.width(), r.height()));
        let _ = b.set_attribute("data-i", &i.to_string());
        if top < 16.0 {
            let _ = b.set_attribute("data-flip", "");
        }
        if let Ok(chip) = d.create_element("i") {
            chip.set_text_content(Some(&i.to_string()));
            let _ = b.append_child(&chip);
        }
        let _ = boxes.append_child(&b);
    }
}

fn reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-reduced-motion: reduce)").ok().flatten())
        .map(|m| m.matches())
        .unwrap_or(false)
}

/// Glide the cursor to the element's center, mark its outline active, wait for the glide.
pub async fn point_at(el: &Element, index: Option<usize>) {
    let Some(c) = q(".cursor") else { return };
    if let Some(prev) = q(".hl[data-active]") {
        let _ = prev.remove_attribute("data-active");
    }
    if let Some(b) = index.and_then(|i| q(&format!(".hl[data-i=\"{i}\"]"))) {
        let _ = b.set_attribute("data-active", "");
    }
    let r = el.get_bounding_client_rect();
    let (ox, oy) = frame_offset(el);
    let (x, y) = (r.left() + ox + r.width() / 2.0, r.top() + oy + r.height() / 2.0);
    if let Some(h) = c.dyn_ref::<web_sys::HtmlElement>() {
        let s = h.style();
        let _ = s.set_property("--x", &format!("{x}px"));
        let _ = s.set_property("--y", &format!("{y}px"));
    }
    crate::actions::sleep(if reduced_motion() { 0.05 } else { 0.45 }).await;
}

pub fn ripple() {
    if let Some(c) = q(".cursor") {
        let _ = c.remove_attribute("data-click");
        let _ = c.get_bounding_client_rect(); // restart the animation
        let _ = c.set_attribute("data-click", "");
    }
}
