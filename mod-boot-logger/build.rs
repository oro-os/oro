#![expect(missing_docs, clippy::missing_docs_in_private_items)]

use std::path::PathBuf;

use rusttype::{Font, Scale};

const FONT_SIZE: f32 = 15.0;

#[expect(
	clippy::cast_sign_loss,
	clippy::cast_possible_truncation,
	clippy::cast_possible_wrap
)]
fn main() {
	let raw_font_path = PathBuf::from(
		std::env::var("CARGO_MANIFEST_DIR").expect("no environment variable 'CARGO_MANIFEST_DIR"),
	)
	.join("AtkinsonHyperlegibleMono-Light.ttf");

	let raw_font = std::fs::read(raw_font_path).expect("failed to read font file");

	let font = Font::try_from_vec(raw_font).expect("failed to load font");

	let v_metrics = font.v_metrics(Scale::uniform(1.0));
	let font_height = (FONT_SIZE * (v_metrics.ascent - v_metrics.descent)).ceil() as usize;

	let mut rows: Vec<Vec<u8>> = Vec::with_capacity(font_height);
	for _ in 0..font_height {
		rows.push(Vec::new());
	}

	let dict = (0..256).map(|c| char::from_u32(c).unwrap());
	let layout = font.glyphs_for(dict).collect::<Vec<_>>();

	let y_baseline = (v_metrics.ascent * FONT_SIZE).ceil() as i32;
	let mut x_base = 0;
	let mut offsets = Vec::new();

	for glyph in layout {
		if glyph.id().0 == 0 {
			offsets.push(u32::MAX);
			continue;
		}

		let glyph = glyph.scaled(Scale::uniform(FONT_SIZE));
		let glyph = glyph.positioned(rusttype::point(0.0, 0.0));

		if let Some(bb) = glyph.pixel_bounding_box() {
			glyph.draw(|x, y, v| {
				let x = x_base + x as usize + bb.min.x.max(0) as usize;
				let y = y as i32 + bb.min.y + y_baseline;

				if y < 0 || y >= font_height as i32 {
					return;
				}

				let y = y as usize;

				let Some(row) = rows.get_mut(y) else {
					return;
				};

				if x >= row.len() {
					row.resize(x + 1, 0);
				}

				let v = ((v.clamp(0.0, 1.0).powf(2.0) * (255.0 / 4.0)).round() * 255.0 / 4.0)
					.clamp(0.0, 255.0)
					.round() as u8;

				row[x] = v;
			});
		}

		offsets.push(x_base as u32);
		assert!(
			glyph
				.unpositioned()
				.h_metrics()
				.advance_width
				.ceil()
				.is_sign_positive()
		);
		x_base += glyph.unpositioned().h_metrics().advance_width.ceil() as usize;
	}

	let max_len = rows.iter().map(std::vec::Vec::len).max().unwrap();
	for row in &mut rows {
		if row.len() < max_len {
			row.resize(max_len, 0);
		}
	}

	let data = rows.into_iter().flatten().collect::<Vec<_>>();

	std::fs::write(
		PathBuf::from(std::env::var("OUT_DIR").expect("no environment variable 'OUT_DIR'"))
			.join("font.bin"),
		&data,
	)
	.expect("failed to write font data to file");

	let metrics = quote::quote! {
		/// The height of the font.
		pub const FONT_HEIGHT: usize = #font_height;

		/// The offsets of each character in the font.
		///
		/// `u32::MAX` indicates that the character is not present in the font.
		#[allow(clippy::unreadable_literal)]
		pub static FONT_OFFSETS: [u32; 256] = [
			#(#offsets),*
		];
	}
	.to_string();

	std::fs::write(
		PathBuf::from(std::env::var("OUT_DIR").expect("no environment variable 'OUT_DIR'"))
			.join("font-metrics.rs"),
		metrics.as_bytes(),
	)
	.expect("failed to write font metrics to file");
}
