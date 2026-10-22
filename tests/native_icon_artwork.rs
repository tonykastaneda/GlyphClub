#![cfg(target_os = "macos")]

use objc2::AnyThread;
use objc2_app_kit::{NSImage, NSPasteboardItem};
use objc2_foundation::{NSData, NSString};

#[test]
fn native_drag_item_accepts_vector_formats_and_pdf_preview() {
    let sets = glyphclub::icons::load_sets();
    assert!(!sets.is_empty());
    for set in sets {
        let font = std::fs::read(&set.font_path).unwrap();
        let icon = &set.icons[0];
        let pdf = glyphclub::icons::icon_pdf(&font, icon.codepoint, 64.).unwrap();
        let svg = glyphclub::icons::icon_svg(&font, icon.codepoint, 64.).unwrap();
        let item = NSPasteboardItem::new();
        let pdf_type = NSString::from_str("com.adobe.pdf");
        let svg_type = NSString::from_str("public.svg-image");
        let data = NSData::with_bytes(&pdf);
        assert!(item.setData_forType(&data, &pdf_type));
        assert!(item.setData_forType(&NSData::with_bytes(svg.as_bytes()), &svg_type));
        assert_eq!(item.dataForType(&pdf_type).unwrap().to_vec(), pdf);
        assert_eq!(item.dataForType(&svg_type).unwrap().to_vec(), svg.as_bytes());
        assert!(NSImage::initWithData(NSImage::alloc(), &data).is_some(), "{}", set.name);
    }
}
