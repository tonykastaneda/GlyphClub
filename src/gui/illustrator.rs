//! "Apply in Illustrator": sets the selected text in Illustrator's front
//! document to a font, by running a small ExtendScript through Apple Events
//! (the same route RightFont uses). macOS asks once for permission to
//! control Illustrator.

use std::process::Command;

pub enum ApplyResult {
    Applied,
    NoDocument,
    NoTextSelected,
    /// Illustrator doesn't know the font yet (it can take a moment to see a
    /// font that was just activated).
    FontNotLoaded,
    NotRunning,
    Failed(String),
}

const SCRIPT: &str = r#"
(function (ps) {
    if (app.documents.length === 0) return "nodoc";
    var font;
    try { font = app.textFonts.getByName(ps); } catch (e) { return "nofont"; }
    var count = 0;
    function apply(item) {
        if (!item) return;
        if (item.typename === "TextFrame") {
            item.textRange.characterAttributes.textFont = font; count++;
        } else if (item.typename === "TextRange") {
            item.characterAttributes.textFont = font; count++;
        } else if (item.typename === "GroupItem") {
            for (var i = 0; i < item.pageItems.length; i++) apply(item.pageItems[i]);
        }
    }
    var sel = app.activeDocument.selection;
    if (sel && sel.typename === "TextRange") {
        apply(sel);
    } else if (sel) {
        for (var i = 0; i < sel.length; i++) apply(sel[i]);
    }
    return count === 0 ? "nosel" : "ok:" + count;
})(__PS__)
"#;

pub fn is_running() -> bool {
    glyphclub::integrations::illustrator_running()
}

/// Blocking; run it off the main thread.
pub fn apply_font(postscript_name: &str) -> ApplyResult {
    if !is_running() {
        return ApplyResult::NotRunning;
    }
    let ps_literal = serde_json::to_string(postscript_name).unwrap_or_else(|_| "\"\"".into());
    let script = SCRIPT.replace("__PS__", &ps_literal);
    let output = Command::new("osascript")
        .arg("-e")
        .arg("on run argv\ntell application id \"com.adobe.illustrator\" to do javascript (item 1 of argv)\nend run")
        .arg(script)
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => return ApplyResult::Failed(e.to_string()),
    };
    let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return ApplyResult::Failed(if err.is_empty() { "Illustrator didn't respond".into() } else { err });
    }
    match out.as_str() {
        "nodoc" => ApplyResult::NoDocument,
        "nosel" => ApplyResult::NoTextSelected,
        "nofont" => ApplyResult::FontNotLoaded,
        s if s.starts_with("ok:") => ApplyResult::Applied,
        other => ApplyResult::Failed(other.to_string()),
    }
}
