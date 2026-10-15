//! Page actions performed with synthetic events, mirroring page-agent's PageController.

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement};

pub async fn sleep(secs: f64) {
    let p = js_sys::Promise::new(&mut |res, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&res, (secs * 1000.0) as i32)
            .unwrap();
    });
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
}

fn scroll_into_view(el: &Element) {
    let o = web_sys::ScrollIntoViewOptions::new();
    o.set_block(web_sys::ScrollLogicalPosition::Center);
    o.set_inline(web_sys::ScrollLogicalPosition::Nearest);
    el.scroll_into_view_with_scroll_into_view_options(&o);
}

fn mouse(target: &Element, kind: &str, x: f64, y: f64, bubbles: bool) {
    let i = web_sys::MouseEventInit::new();
    i.set_bubbles(bubbles);
    i.set_cancelable(true);
    i.set_client_x(x as i32);
    i.set_client_y(y as i32);
    i.set_button(0);
    if let Ok(e) = web_sys::MouseEvent::new_with_mouse_event_init_dict(kind, &i) {
        let _ = target.dispatch_event(&e);
    }
}

fn pointer(target: &Element, kind: &str, x: f64, y: f64, bubbles: bool) {
    let i = web_sys::PointerEventInit::new();
    i.set_bubbles(bubbles);
    i.set_cancelable(true);
    i.set_client_x(x as i32);
    i.set_client_y(y as i32);
    i.set_pointer_type("mouse");
    if let Ok(e) = web_sys::PointerEvent::new_with_event_init_dict(kind, &i) {
        let _ = target.dispatch_event(&e);
    }
}

fn plain_event(target: &Element, kind: &str) {
    let i = web_sys::EventInit::new();
    i.set_bubbles(true);
    if let Ok(e) = web_sys::Event::new_with_event_init_dict(kind, &i) {
        let _ = target.dispatch_event(&e);
    }
}

pub async fn click(el: &Element) -> Result<(), String> {
    scroll_into_view(el);
    sleep(0.05).await;
    let r = el.get_bounding_client_rect();
    let (x, y) = (r.left() + r.width() / 2.0, r.top() + r.height() / 2.0);
    let doc = web_sys::window().unwrap().document().unwrap();
    // dispatch on the deepest element under the pointer if it belongs to the target
    let t = doc
        .element_from_point(x as f32, y as f32)
        .filter(|h| el.contains(Some(h.as_ref())))
        .unwrap_or_else(|| el.clone());
    pointer(&t, "pointerover", x, y, true);
    pointer(&t, "pointerenter", x, y, false);
    mouse(&t, "mouseover", x, y, true);
    mouse(&t, "mouseenter", x, y, false);
    pointer(&t, "pointerdown", x, y, true);
    mouse(&t, "mousedown", x, y, true);
    if let Some(h) = el.dyn_ref::<HtmlElement>() {
        let f = web_sys::FocusOptions::new();
        f.set_prevent_scroll(true);
        let _ = h.focus_with_options(&f);
    }
    pointer(&t, "pointerup", x, y, true);
    mouse(&t, "mouseup", x, y, true);
    t.dyn_ref::<HtmlElement>().ok_or("not an HTMLElement")?.click();
    sleep(0.2).await;
    Ok(())
}

/// Set value through the prototype's native setter so React/Vue controlled inputs notice.
fn native_set_value(el: &Element, v: &str) -> Result<(), String> {
    let proto = js_sys::Object::get_prototype_of(el.as_ref());
    let desc = js_sys::Object::get_own_property_descriptor(&proto, &JsValue::from_str("value"));
    let setter = js_sys::Reflect::get(&desc, &JsValue::from_str("set")).map_err(|_| "no value setter")?;
    setter
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "no value setter")?
        .call1(el.as_ref(), &JsValue::from_str(v))
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

pub async fn input_text(el: &Element, text: &str) -> Result<(), String> {
    let h = el.dyn_ref::<HtmlElement>().ok_or("not an HTMLElement")?;
    let editable = h.is_content_editable();
    let is_field = el.dyn_ref::<web_sys::HtmlInputElement>().is_some() || el.dyn_ref::<web_sys::HtmlTextAreaElement>().is_some();
    if !is_field && !editable {
        return Err("Element is not an input, textarea, or contenteditable".into());
    }
    click(el).await?;
    if editable {
        h.set_inner_text(text);
        plain_event(el, "input");
        plain_event(el, "change");
        let _ = h.blur();
    } else {
        native_set_value(el, text)?;
        plain_event(el, "input");
        plain_event(el, "change");
    }
    sleep(0.1).await;
    Ok(())
}

pub async fn select_option(el: &Element, text: &str) -> Result<(), String> {
    let s = el.dyn_ref::<web_sys::HtmlSelectElement>().ok_or("Element is not a select element")?;
    let opts = s.options();
    let opt = (0..opts.length())
        .filter_map(|i| opts.item(i))
        .filter_map(|o| o.dyn_into::<web_sys::HtmlOptionElement>().ok())
        .find(|o| o.text().trim() == text.trim())
        .ok_or_else(|| format!("Option with text \"{text}\" not found in select element"))?;
    s.set_value(&opt.value());
    plain_event(el, "change");
    sleep(0.1).await;
    Ok(())
}

/// Scroll `px` (signed). With an element: nearest scrollable ancestor. Without: the page,
/// falling back to the largest scrollable container (SPAs with fixed bodies).
pub async fn scroll(el: Option<&Element>, px: f64, vertical: bool) -> String {
    let win = web_sys::window().unwrap();
    let doc = win.document().unwrap();
    let pos = |e: &Element| if vertical { e.scroll_top() } else { e.scroll_left() };
    let set = |e: &Element, v: i32| if vertical { e.set_scroll_top(v) } else { e.set_scroll_left(v) };
    let can = |e: &Element| {
        let ov = win
            .get_computed_style(e)
            .ok()
            .flatten()
            .and_then(|s| s.get_property_value(if vertical { "overflow-y" } else { "overflow-x" }).ok())
            .unwrap_or_default();
        ["auto", "scroll", "overlay"].contains(&ov.as_str())
            && if vertical { e.scroll_height() > e.client_height() } else { e.scroll_width() > e.client_width() }
    };
    let try_container = |start: &Element| -> Option<String> {
        let mut cur = Some(start.clone());
        for _ in 0..10 {
            let e = cur?;
            if can(&e) {
                let before = pos(&e);
                set(&e, before + px as i32);
                let moved = pos(&e) - before;
                if moved != 0 {
                    return Some(format!("✅ Scrolled container ({}) by {moved}px", e.tag_name()));
                }
            }
            cur = e.parent_element();
        }
        None
    };
    if let Some(e) = el {
        return try_container(e).unwrap_or_else(|| format!("No scrollable container found for element ({})", e.tag_name()));
    }
    let before = if vertical { win.scroll_y() } else { win.scroll_x() }.unwrap_or(0.0);
    if vertical { win.scroll_by_with_x_and_y(0.0, px) } else { win.scroll_by_with_x_and_y(px, 0.0) }
    let moved = if vertical { win.scroll_y() } else { win.scroll_x() }.unwrap_or(0.0) - before;
    if moved.abs() >= 1.0 {
        return format!("✅ Scrolled page by {moved}px.");
    }
    // ponytail: O(n) querySelectorAll scan for a big scroll container; fine for typical pages
    let vh = win.inner_height().unwrap().as_f64().unwrap_or(0.0);
    if let Ok(all) = doc.query_selector_all("*") {
        for i in 0..all.length() {
            if let Some(e) = all.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                if e.client_height() as f64 >= vh * 0.5 && can(&e) {
                    if let Some(m) = try_container(&e) {
                        return m;
                    }
                }
            }
        }
    }
    let edge = if px > 0.0 { if vertical { "bottom" } else { "right" } } else if vertical { "top" } else { "left" };
    format!("⚠️ Already at the {edge} of the page, cannot scroll further.")
}

pub async fn execute_js(script: &str) -> String {
    let f = js_sys::Function::new_no_args(&format!("return (async () => {{ {script} }})()"));
    let run = async {
        let p: js_sys::Promise = f.call0(&JsValue::NULL)?.dyn_into()?;
        wasm_bindgen_futures::JsFuture::from(p).await
    };
    match run.await {
        Ok(v) => format!(
            "✅ Executed JavaScript. Result: {}",
            js_sys::JSON::stringify(&v).ok().and_then(|s| s.as_string()).unwrap_or_else(|| "undefined".into())
        ),
        Err(e) => format!("❌ Error executing JavaScript: {}", e.as_string().unwrap_or_else(|| format!("{e:?}"))),
    }
}
