//! Boot time logger module for the Oro operating system.
//!
//! This module does its best to provide graphical (or at least
//! visual) logging output during the earlier boot stages of
//! the Oro operating system.
#![expect(clippy::many_single_char_names)]

use oro::{
	LazyIfaceId,
	id::iface::{KERNEL_IFACE_QUERY_TYPE_META_V0, ROOT_BOOT_VBUF_V0, ROOT_DEBUG_OUT_V0},
	key,
	syscall::Error,
	syscall_get, syscall_set,
};
use oro_logo_rle::{Command, OroLogoData};

mod font_rasterizer;

/// The Oro logo, aliased to a specific resolution.
type OroLogo = oro_logo_rle::OroLogo<oro_logo_rle::OroLogo64x64>;

/// How many steps to fade in per frame.
const FADE_IN_STEP: u8 = 2;

/// Lightness values mapped to grey RGB values.
const LIGHTNESSES: [u8; 4] = [0, 0x55, 0xAA, 0xFF];

/// A video buffer object.
///
/// This is a very basic representation of the internal kernel video buffer
/// structures, and assumes a number of things (such as the buffer being
/// RGB and 8 bits per channel).
struct Vbuf {
	/// The number of pixels per row.
	///
	/// **Note:** Do not assume `y * width * bytes_per_pixel` will give you
	/// the correct base line offset. Padding bytes might be present.
	/// Multiply `width * stride` instead (_not_ multiplying by `bytes_per_pixel`).
	width: u64,
	/// The number of rows.
	height: u64,
	/// The number of bytes per row. This may not be equal to `width * bytes_per_pixel`,
	/// as padding bytes might be present.
	stride: u64,
	/// The number of _bits_ per pixel.
	bits_per_pixel: u64,
	/// The number of _bytes_ per pixel.
	bytes_per_pixel: u64,
	/// The number of bits per red channel within a pixel.
	red_mask: u64,
	/// The number of bits per green channel within a pixel.
	green_mask: u64,
	/// The number of bits per blue channel within a pixel.
	blue_mask: u64,
	/// The base virtual address of the video buffer.
	data: *mut u8,
}

/// The root ring debug output interface ID.
static DEBUG_OUT_IFACE: LazyIfaceId<ROOT_DEBUG_OUT_V0> = LazyIfaceId::new();
/// The root ring video buffer interface ID.
static VBUF_IFACE: LazyIfaceId<ROOT_BOOT_VBUF_V0> = LazyIfaceId::new();

/// Attempts to fetch information for, and map in, a video buffer from the kernel
/// given its index.
///
/// Returns an error if any of the syscalls fail.
fn find_video_buffer(idx: u64) -> Result<Vbuf, (Error, u64)> {
	// SAFETY: This is inherently unsafe but we're following the
	// SAFETY: guidelines for syscalls.
	unsafe {
		let root_vbuf_iface = VBUF_IFACE
			.get()
			.expect("failed to retrieve root ring video buffer interface");

		#[doc(hidden)]
		macro_rules! get_vbuf_field {
			($field:literal) => {{ syscall_get!(ROOT_BOOT_VBUF_V0, root_vbuf_iface, idx, key!($field),)? }};
		}

		let vbuf_addr: u64 = 0x3C00_0000_0000 + idx * 0x1_0000_0000;

		let bits_per_pixel = get_vbuf_field!("bit_pp");
		let bytes_per_pixel = bits_per_pixel / 8;

		Ok(Vbuf {
			width: get_vbuf_field!("width"),
			height: get_vbuf_field!("height"),
			bits_per_pixel,
			bytes_per_pixel,
			stride: get_vbuf_field!("pitch"),
			red_mask: get_vbuf_field!("red_size"),
			green_mask: get_vbuf_field!("grn_size"),
			blue_mask: get_vbuf_field!("blu_size"),
			data: {
				syscall_set!(
					ROOT_BOOT_VBUF_V0,
					root_vbuf_iface,
					idx,
					key!("!vmbase!"),
					vbuf_addr
				)?;

				vbuf_addr as *mut u8
			},
		})
	}
}

impl Vbuf {
	/// Sets a pixel to a grey level.
	fn set_grey_pixel(&self, x: u64, y: u64, level: u8) {
		if x >= self.width || y >= self.height {
			return;
		}

		unsafe {
			self.set_grey_pixel_unchecked(x, y, level);
		}
	}

	/// Sets a pixel to a grey level, without checking bounds.
	///
	/// # Safety
	/// Does not check if `x` or `x` are beyond the bounds of the buffer.
	unsafe fn set_grey_pixel_unchecked(&self, x: u64, y: u64, level: u8) {
		unsafe {
			#[expect(clippy::cast_possible_wrap)]
			let base = self
				.data
				.offset(((y * self.stride) + (x * self.bytes_per_pixel)) as isize);
			*base = level;
			*(base.offset(1)) = level;
			*(base.offset(2)) = level;
		}
	}

	/// Draws a vertical line.
	fn draw_vline(&self, x: u64, y1: u64, y2: u64, level: u8) {
		if x >= self.width || y1 >= self.height {
			return;
		}

		let y2 = y2.clamp(y1, self.height - 1);

		for y in y1..=y2 {
			// SAFETY: We properly check the bounds of the draw above.
			unsafe {
				self.set_grey_pixel_unchecked(x, y, level);
			}
		}
	}

	/// Draws a horizontal line.
	fn draw_hline(&self, x1: u64, x2: u64, y: u64, level: u8) {
		if x1 >= self.width || y >= self.height {
			return;
		}

		let x2 = x2.clamp(x1, self.width - 1);

		for x in x1..=x2 {
			// SAFETY: We properly check the bounds of the draw above.
			unsafe {
				self.set_grey_pixel_unchecked(x, y, level);
			}
		}
	}

	/// Draws a box.
	fn draw_box(&self, x1: u64, y1: u64, x2: u64, y2: u64, level: u8) {
		self.draw_hline(x1, x2, y1, level);
		self.draw_hline(x1, x2, y2, level);
		self.draw_vline(x1, y1, y2, level);
		self.draw_vline(x2, y1, y2, level);
	}

	/// Fills an area with a level.
	fn fill_box(&self, x1: u64, y1: u64, x2: u64, y2: u64, level: u8) {
		if x1 >= self.width || y1 >= self.height {
			return;
		}

		let x2 = x2.clamp(x1, self.width - 1);
		let y2 = y2.clamp(y1, self.height - 1);

		for y in y1..=y2 {
			for x in x1..=x2 {
				// SAFETY: We properly check the bounds of the draw above.
				unsafe {
					self.set_grey_pixel_unchecked(x, y, level);
				}
			}
		}
	}
}

// Sleeps between a frame.
//
// NOTE(qix-): Temporary function. Please do not copy into your modules.
#[doc(hidden)]
fn sleep_between_frame() {
	// TODO(qix-): Implement a proper sleep syscall.
	for _ in 0..1_000_000 {
		unsafe {
			core::arch::asm!("nop");
		}
	}
}

fn main() {
	// SAFETY: Just a query, always safe.
	match unsafe {
		syscall_get!(
			KERNEL_IFACE_QUERY_TYPE_META_V0,
			KERNEL_IFACE_QUERY_TYPE_META_V0,
			ROOT_BOOT_VBUF_V0,
			key!("icount")
		)
	} {
		Ok(ifaces) => {
			println!("ring has {ifaces} ROOT_BOOT_VBUF_V0 interface(s)");
		}
		Err((err, ext)) => {
			println!(
				"could not get ROOT_BOOT_VBUF_V0 interface count: {err:?}[{:?}]",
				::oro::Key(&ext)
			);
			return;
		}
	}

	println!("looking for vbuf 0...");

	let vbuf = match find_video_buffer(0) {
		Ok(vbuf) => {
			println!("found vbuf 0");
			vbuf
		}
		Err((err, ext)) => {
			println!("failed to find vbuf 0: {err:?}[{:?}]", ::oro::Key(&ext));
			return;
		}
	};

	if (vbuf.bits_per_pixel & 0b111) != 0 {
		println!("vbuf 0 is not byte-aligned");
		return;
	}

	if vbuf.red_mask != 8 {
		println!("vbuf 0 red channel is not 8 bits");
		return;
	}

	if vbuf.green_mask != 8 {
		println!("vbuf 0 green channel is not 8 bits");
		return;
	}

	if vbuf.blue_mask != 8 {
		println!("vbuf 0 blue channel is not 8 bits");
		return;
	}

	vbuf.draw_box(3, 3, vbuf.width - 3, vbuf.height - 3, 0x77);

	let left = vbuf.width - (OroLogo::WIDTH as u64) - 5;
	let top = vbuf.height - (OroLogo::HEIGHT as u64) - 5;

	let text_right: usize = vbuf.width as usize - 15 - OroLogo::WIDTH;
	let text_left: usize = 15;
	let text_top: usize = 5;
	let text_bottom: usize = vbuf.height as usize - 5;

	let mut iter = OroLogo::new();

	let mut fade_in = 255u8;

	let mut text_x: usize = 0;
	let mut text_y: usize = 0;

	let mut cursor_y = 0;
	let mut last_cursor_y = 0;
	let mut cursor_level = (101u8..=255u8)
		.chain((100u8..=254u8).rev())
		.cycle()
		.step_by(7);

	loop {
		let mut off = 0usize;

		#[doc(hidden)]
		static mut OFF_SCREEN: [u8; (OroLogo::WIDTH * OroLogo::HEIGHT) / 4] =
			[0; { (OroLogo::WIDTH * OroLogo::HEIGHT) / 4 }];

		fade_in = fade_in.saturating_sub(FADE_IN_STEP);

		loop {
			match iter.next() {
				None => {
					println!("Oro logo exhausted commands (shouldn't happen)");
					return;
				}

				Some(Command::End) => break,

				Some(Command::Draw(count, lightness)) => {
					if fade_in > 0 {
						// We need to draw first to the off-screen buffer,
						// then blit it to the screen with the multiplier.
						for i in 0..count {
							let off = off + i as usize;
							let byte_off = off / 4;
							let bit_off = (off % 4) * 2;
							unsafe {
								OFF_SCREEN[byte_off] = OFF_SCREEN[byte_off] & !(0b11 << bit_off)
									| ((lightness & 0b11) << bit_off);
							}
						}
					} else {
						// Otherwise, we can draw directly.
						let color = LIGHTNESSES[(lightness & 0b11) as usize];

						for i in 0..count {
							let off = off + i as usize;
							let x = off % OroLogo::WIDTH;
							let y = off / OroLogo::WIDTH;
							let x = x as u64 + left;
							let y = y as u64 + top;
							vbuf.set_grey_pixel(x, y, color);
						}
					}

					off += count as usize;
				}

				Some(Command::Skip(count)) => {
					off += count as usize;
				}
			}
		}

		// If we're fading in, we need to blit the off-screen buffer to the screen.
		if fade_in > 0 {
			let mut off = 0usize;

			for _ in 0..OroLogo::HEIGHT {
				for _ in 0..OroLogo::WIDTH {
					let byte_off = off / 4;
					let bit_off = (off % 4) * 2;
					let lightness = unsafe { OFF_SCREEN[byte_off] >> bit_off } & 0b11;
					let color = LIGHTNESSES[lightness as usize];
					let color = color.saturating_sub(fade_in);

					let x = off % OroLogo::WIDTH;
					let y = off / OroLogo::WIDTH;
					let x = x as u64 + left;
					let y = y as u64 + top;
					vbuf.set_grey_pixel(x, y, color);

					off += 1;
				}
			}
		}

		// Now rasterize the root ring logs.
		if let Some(debug_iface) = DEBUG_OUT_IFACE.get() {
			loop {
				// SAFETY: This is always safe.
				let Ok(r) =
					(unsafe { syscall_get!(ROOT_DEBUG_OUT_V0, debug_iface, 0, key!("ring_u64")) })
				else {
					break;
				};

				if r == 0 {
					break;
				}

				for shift in (0..=(64 - 8)).rev().step_by(8) {
					let b = ((r >> shift) & 0xFF) as u8;
					if b == 0 {
						break;
					}
					let c = b as char;

					if c == '\n' {
						text_x = 0;
						text_y += 1;

						if ((text_y + 1) * font_rasterizer::LINE_HEIGHT) >= text_bottom as usize {
							text_y = 0;
						}

						continue;
					}

					if text_x >= text_right {
						continue;
					}

					let iter = font_rasterizer::render_glyph(c)
						.or_else(|| font_rasterizer::render_glyph('?'))
						.expect("missing glyph");

					let xoff = text_x;
					let width = iter.width();

					if width > 0 {
						text_x += width as usize;
					}

					if xoff == 0 {
						// First write of the line; clear it.
						let left = text_left;
						let right = text_right;
						let top = text_top + (text_y * font_rasterizer::LINE_HEIGHT);
						let bottom = top + font_rasterizer::LINE_HEIGHT;
						vbuf.fill_box(left as u64, top as u64, right as u64, bottom as u64, 0);
						cursor_y = text_y;
					}

					for (x, y, v) in iter {
						let x = text_left + x + xoff;
						let y = text_top + y + (text_y * font_rasterizer::LINE_HEIGHT);
						if x < text_right && y < text_bottom {
							vbuf.set_grey_pixel(x as u64, y as u64, v);
						}
					}
				}
			}
		}

		// Now the cursor.
		let cursor_left = 5;
		let cursor_top = cursor_y * font_rasterizer::LINE_HEIGHT + text_top;
		let cursor_bottom = cursor_top + font_rasterizer::LINE_HEIGHT;
		let cursor_right = 10;

		if last_cursor_y != cursor_y {
			// Clear the old cursor
			let cursor_top = last_cursor_y * font_rasterizer::LINE_HEIGHT + text_top;
			let cursor_bottom = cursor_top + font_rasterizer::LINE_HEIGHT;
			vbuf.fill_box(
				cursor_left,
				cursor_top as u64,
				cursor_right,
				cursor_bottom as u64,
				0,
			);
			last_cursor_y = cursor_y;
		}

		vbuf.fill_box(
			cursor_left,
			cursor_top as u64,
			cursor_right,
			cursor_bottom as u64,
			cursor_level.next().unwrap_or(255),
		);

		sleep_between_frame(/*1000 / OroLogo::FPS as u64*/);
	}
}
