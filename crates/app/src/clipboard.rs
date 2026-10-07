//! The system clipboard, on both targets.
//!
//! One implementation rather than two, through [`document::eval`]: the desktop build is a webview
//! with a DOM, so the browser's own clipboard is the clipboard there too. A `cfg(target_arch =
//! "wasm32")` helper with a `None` twin would be this feature DELETED on the desktop, which is a
//! trap this codebase has already fallen into once.
//!
//! **Two mechanisms, because neither is available everywhere.** `navigator.clipboard` needs a
//! secure context and, in most browsers, a live user gesture; the older selection-and-copy path
//! needs neither but is refused outright by some. Both are tried, and if both refuse the caller
//! is TOLD — a copy button that reports success while the clipboard still holds what it held
//! before is the soft-wrong outcome Rule 1 exists to prevent, and the user finds out by pasting.

use dioxus::prelude::*;

/// Put `text` on the clipboard, or say why not.
///
/// Must be called from a user gesture's own handler: both mechanisms below are gated on one in at
/// least one browser, and the gesture is over by the time a later task runs.
pub async fn copy(text: &str) -> Result<(), String> {
    // As a JSON literal rather than through `{:?}`: Rust's debug escaping is close enough to
    // JavaScript's to be tempting and is not the same language, and this payload is a whole
    // document of user-authored text.
    let payload = serde_json::to_string(text).map_err(|e| e.to_string())?;
    let js = format!(
        "const text = {payload};\
         try {{\
           if (navigator.clipboard && window.isSecureContext) {{\
             await navigator.clipboard.writeText(text);\
             return '';\
           }}\
         }} catch (e) {{ /* fall through to the selection path */ }}\
         try {{\
           const box = document.createElement('textarea');\
           box.value = text;\
           box.setAttribute('readonly', '');\
           box.style.position = 'fixed';\
           box.style.top = '-1000px';\
           box.style.opacity = '0';\
           document.body.appendChild(box);\
           box.select();\
           box.setSelectionRange(0, box.value.length);\
           const ok = document.execCommand('copy');\
           document.body.removeChild(box);\
           return ok ? '' : 'this browser refused the copy';\
         }} catch (e) {{ return String(e); }}"
    );
    let value = document::eval(&js).await.map_err(|e| format!("{e:?}"))?;
    match value.as_str().unwrap_or_default() {
        "" => Ok(()),
        message => Err(message.to_string()),
    }
}

/// Put a PNG on the clipboard as an image, or say why not.
///
/// Same gesture rule as [`copy`]. Unlike text there is no second mechanism to fall back to: the
/// selection-and-`execCommand` path can only carry text, so a browser without `ClipboardItem`
/// simply cannot do this and the caller is told so rather than shown a success it did not get.
pub async fn copy_png(bytes: &[u8]) -> Result<(), String> {
    // The bytes reach JS as base64 rather than as an array literal: a poster is a megabyte or
    // two, and a literal of that many comma-separated integers is several times the size and has
    // to be parsed as source.
    let payload = serde_json::to_string(&base64(bytes)).map_err(|e| e.to_string())?;
    let js = format!(
        "if (!window.ClipboardItem || !navigator.clipboard || !navigator.clipboard.write) {{\
           return 'this browser cannot put an image on the clipboard';\
         }}\
         try {{\
           const bin = atob({payload});\
           const bytes = new Uint8Array(bin.length);\
           for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);\
           const blob = new Blob([bytes], {{ type: 'image/png' }});\
           await navigator.clipboard.write([new ClipboardItem({{ 'image/png': blob }})]);\
           return '';\
         }} catch (e) {{ return String(e); }}"
    );
    let value = document::eval(&js).await.map_err(|e| format!("{e:?}"))?;
    match value.as_str().unwrap_or_default() {
        "" => Ok(()),
        message => Err(message.to_string()),
    }
}

/// Standard base64, no line breaks. Hand-rolled because binary-to-JavaScript is the only place
/// this app needs it and a crate would be the whole dependency for these twenty lines. Shared
/// with [`crate::build_file::write_png_file`], which hands the browser a blob the same way.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            // The tail is padded with '=' for however many source bytes were missing.
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - i * 6) & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
