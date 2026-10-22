# GPUI native artwork drag patch

These Apache-2.0 licensed crates are vendored from gpui-pre 0.3.9
(zed revision 93d745668805029476e6d7fbb19652116fed51de).
Their original license files are preserved alongside the sources.

Local changes:
- gpui-pre: adds macOS-only ExternalDragPayload::Data representations.
- gpui-pre-macos: creates an NSPasteboardItem containing those representations
  and starts a native NSDraggingSession with a PDF artwork preview.
- Existing file dragging and other platforms keep their original behavior.

GlyphClub uses this to offer com.adobe.pdf and public.svg-image directly,
without file URLs or temporary files. Keep these changes when updating GPUI.
