//! Text: parley lays glyphs out, vello rasterizes them. One [`TextCx`] per
//! app, kept alive for its lifetime — it owns the font database (including
//! every font file the catalog has loaded a live preview for) and parley's
//! reusable shaping buffers.

use std::sync::Arc;

use parley::fontique::Blob;
use parley::{
    FontContext, FontFamily, GenericFamily, Layout, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};
use vello::kurbo::Affine;
use vello::peniko::{Brush, Color, Fill};
use vello::{Glyph, Scene};

pub struct TextCx {
    fonts: FontContext,
    layout_cx: LayoutContext<Brush>,
}

impl TextCx {
    pub fn new() -> Self {
        Self {
            fonts: FontContext::new(),
            layout_cx: LayoutContext::new(),
        }
    }

    /// Registers a font file's raw bytes with the font database and returns
    /// the family name fontique parsed out of it. Pass that name to
    /// [`Self::draw`]/[`Self::measure`] via `family` to target this exact
    /// font — this is the catalog's live-preview path: nothing here reads
    /// from a system font list.
    pub fn register_font_bytes(&mut self, bytes: Vec<u8>) -> Option<String> {
        let blob = Blob::new(Arc::new(bytes));
        let registered = self.fonts.collection.register_fonts(blob, None);
        let (family_id, _) = registered.into_iter().next()?;
        self.fonts
            .collection
            .family(family_id)
            .map(|f| f.name().to_string())
    }

    fn build(
        &mut self,
        text: &str,
        size: f32,
        family: Option<&str>,
        color: Color,
    ) -> Layout<Brush> {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(size));
        match family {
            Some(name) => builder.push_default(StyleProperty::FontFamily(FontFamily::named(name))),
            None => builder.push_default(GenericFamily::SansSerif),
        }
        builder.push_default(StyleProperty::Brush(Brush::Solid(color)));
        let mut layout: Layout<Brush> = builder.build(text);
        layout.break_all_lines(None);
        layout
    }

    /// Advance width of `text` at `size`, logical px. Used to size and
    /// caret-position the search field and other text controls.
    pub fn measure(&mut self, text: &str, size: f32, family: Option<&str>) -> f64 {
        self.build(text, size, family, Color::WHITE).width() as f64
    }

    /// Draws `text`'s left edge / alphabetic baseline at `(x, y)`.
    pub fn draw(
        &mut self,
        scene: &mut Scene,
        text: &str,
        size: f32,
        family: Option<&str>,
        color: Color,
        x: f64,
        y: f64,
    ) {
        if text.is_empty() {
            return;
        }
        let layout = self.build(text, size, family, color);
        emit(scene, &layout, color, Affine::translate((x, y)));
    }

    /// Draws `text` with its glyph box (ascent+descent, not the taller
    /// line-height box `draw` positions by) centered between `y0` and
    /// `y1` — for labels that sit in a fixed-height control alongside
    /// other labels at a different size, where hand-picked per-call
    /// baseline offsets drift out of alignment with each other the moment
    /// either size changes.
    pub fn draw_centered_v(
        &mut self,
        scene: &mut Scene,
        text: &str,
        size: f32,
        family: Option<&str>,
        color: Color,
        x: f64,
        y0: f64,
        y1: f64,
    ) {
        if text.is_empty() {
            return;
        }
        let layout = self.build(text, size, family, color);
        let metrics = layout.lines().next().map(|l| *l.metrics());
        // `baseline` is the run's own top-to-baseline offset (whatever
        // leading it includes); subtracting it back out after placing the
        // baseline by ascent/descent is what keeps this exact regardless
        // of how that leading is distributed.
        let (ascent, descent, baseline) = metrics
            .map_or((size as f64 * 0.8, 0.0, size as f64 * 0.8), |m| {
                (m.ascent as f64, m.descent as f64, m.baseline as f64)
            });
        let baseline_y = (y0 + y1) / 2.0 + (ascent - descent) / 2.0;
        let top = baseline_y - baseline;
        emit(scene, &layout, color, Affine::translate((x, top)));
    }

    /// Draws `text` with its baseline pinned to `y`, left edge at `x` —
    /// unlike `draw`, which positions the *top* of the layout box, so two
    /// different fonts at the same nominal `size` (different ascent
    /// ratios) drift onto different baselines. Use this whenever several
    /// live font previews sit side by side and are meant to share one
    /// visual ground line, e.g. a specimen grid.
    pub fn draw_at_baseline(
        &mut self,
        scene: &mut Scene,
        text: &str,
        size: f32,
        family: Option<&str>,
        color: Color,
        x: f64,
        y: f64,
    ) {
        if text.is_empty() {
            return;
        }
        let layout = self.build(text, size, family, color);
        let baseline = layout
            .lines()
            .next()
            .map_or(size as f64 * 0.8, |l| l.metrics().baseline as f64);
        emit(scene, &layout, color, Affine::translate((x, y - baseline)));
    }
}

fn emit(scene: &mut Scene, layout: &Layout<Brush>, color: Color, transform: Affine) {
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let mut gx = glyph_run.offset();
            let gy = glyph_run.baseline();
            let run = glyph_run.run();
            let font = run.font();
            let font_size = run.font_size();
            let coords = run.normalized_coords();
            scene
                .draw_glyphs(font)
                .brush(&Brush::Solid(color))
                .hint(false)
                .transform(transform)
                .font_size(font_size)
                .normalized_coords(coords)
                .draw(
                    Fill::NonZero,
                    glyph_run.glyphs().map(|g| {
                        let gx0 = gx + g.x;
                        let gy0 = gy - g.y;
                        gx += g.advance;
                        Glyph {
                            id: g.id as u32,
                            x: gx0,
                            y: gy0,
                        }
                    }),
                );
        }
    }
}
