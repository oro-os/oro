//! Implements the font rasterizer and layout engine.

/// The font to load and use.
static FONT_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/font.bin"));

include!(concat!(env!("OUT_DIR"), "/font-metrics.rs"));

/// The number of pixels in a single row of the font data.
///
/// `FONT_HEIGHT` is statically checked to be an even multiple of
/// this value.
const FONT_DATA_ROW_WIDTH: usize = FONT_DATA.len() / FONT_HEIGHT;

const _: () = {
	assert!(
		FONT_DATA.len() % FONT_HEIGHT == 0,
		"font data is not a multiple of the font height"
	);
};

/// Renders a glyph to a linear buffer with the given width and height,
/// at the given position.
///
/// The callback is called for each pixel with its alpha value
/// (0 being transparent, 255 being opaque) and x/y position relative
/// to the glyph origin.
///
/// Y increases downwards, and is guaranteed to be less than `FONT_HEIGHT`.
///
/// Returns `None` if the glyph is not present in the font.
pub fn render_glyph(c: char) -> Option<GlyphIterator> {
	let offset = FONT_OFFSETS[c as usize];
	if offset == u32::MAX {
		return None;
	}

	let offset = usize::try_from(offset).unwrap();

	let next_offset = FONT_OFFSETS
		.get(c as usize + 1)
		.copied()
		.map_or(FONT_DATA_ROW_WIDTH, |o| usize::try_from(o).unwrap());

	Some(GlyphIterator {
		x_offset: offset,
		width:    next_offset - offset,
		offset:   0,
		total:    FONT_HEIGHT * (next_offset - offset),
	})
}

/// Iterates over the pixels of a glyph.
pub struct GlyphIterator {
	/// The X offset for each row in the glyph data.
	x_offset: usize,
	/// The width of the glyph, in pixels.
	width:    usize,
	/// The current offset into the glyph data (absolute).
	offset:   usize,
	/// The total number of pixels in the glyph.
	total:    usize,
}

impl GlyphIterator {
	/// Returns the width of the glyph.
	pub fn width(&self) -> usize {
		self.width
	}
}

impl Iterator for GlyphIterator {
	type Item = (usize, usize, u8);

	fn next(&mut self) -> Option<Self::Item> {
		if self.offset >= self.total {
			return None;
		}

		let x = self.offset % self.width;
		let y = self.offset / self.width;

		let byte = FONT_DATA[self.x_offset + y * FONT_DATA_ROW_WIDTH + x];
		self.offset += 1;
		Some((x, y, byte))
	}
}
