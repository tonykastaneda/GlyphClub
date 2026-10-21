//! Previews never show a letter cut in half at the edge: text is measured in
//! its own font and only the characters that fit whole are drawn.

use gpui_kit::*;

/// `text` cut down to the characters that fit entirely within `max_width`
/// when set in `font` at `size`. gpui caches shaped lines, so calling this
/// every frame for the visible previews is cheap.
pub fn fit(window: &Window, text: &SharedString, font: Font, size: f32, max_width: f32) -> SharedString {
    if text.is_empty() || max_width <= 0. {
        return SharedString::default();
    }
    let run = TextRun {
        len: text.len(),
        font,
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(text.clone(), px(size), &[run], None);
    if f32::from(line.width) <= max_width {
        return text.clone();
    }
    // The character straddling the edge, and everything after it, goes.
    match line.index_for_x(px(max_width)) {
        Some(i) if text.is_char_boundary(i) => text[..i].trim_end().to_string().into(),
        Some(_) => SharedString::default(),
        None => text.clone(),
    }
}

pub fn preview_font(family: &SharedString, weight: u16, italic: bool, features: FontFeatures) -> Font {
    Font {
        family: family.clone(),
        features,
        fallbacks: None,
        weight: FontWeight(weight as f32),
        style: if italic { FontStyle::Italic } else { FontStyle::Normal },
    }
}
