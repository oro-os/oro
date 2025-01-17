//! Boot time logger module for the Oro operating system.
//!
//! This module does its best to provide graphical (or at least
//! visual) logging output during the earlier boot stages of
//! the Oro operating system.
#![no_main]

use std::os::oro::{
	id::kernel::iface::{ROOT_BOOT_VBUF_V0, ROOT_DEBUG_OUT_V0},
	sysabi, syscall,
};

use oro_logo_rle::{Command, OroLogoData};

/// The Oro logo, aliased to a specific resolution.
type OroLogo = oro_logo_rle::OroLogo<oro_logo_rle::OroLogo256x256>;

/// How many steps to fade in per frame.
const FADE_IN_STEP: u8 = 2;

/// Writes a byte slice to the debug output.
// NOTE(qix-): This module is intended to be an `oro-std` module, which will
// NOTE(qix-): (eventually) have `println!` et al. For now, we hack it in,
// NOTE(qix-): so please do not copy this into your modules.
fn write_bytes(bytes: &[u8]) {
	if bytes.is_empty() {
		return;
	}

	for chunk in bytes.chunks(8) {
		let mut word = 0u64;
		for b in chunk {
			word = (word << 8) | u64::from(*b);
		}

		// XXX(qix-): Hard coding the ID for a moment, bear with.
		syscall::set!(
			ROOT_DEBUG_OUT_V0,
			4_294_967_296,
			0,
			syscall::key!("write"),
			word
		)
		.unwrap();
	}
}

/// Writes a string to the debug output.
// NOTE(qix-): This module is intended to be an `oro-std` module, which will
// NOTE(qix-): (eventually) have `println!` et al. For now, we hack it in,
// NOTE(qix-): so please do not copy this into your modules.
fn write_str(s: &str) {
	write_bytes(s.as_bytes());
}

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

/// Attempts to fetch information for, and map in, a video buffer from the kernel
/// given its index.
///
/// Returns an error if any of the syscalls fail.
fn find_video_buffer(idx: u64) -> Result<Vbuf, (sysabi::syscall::Error, u64)> {
	#[doc(hidden)]
	macro_rules! get_vbuf_field {
		($field:literal) => {{
			syscall::get!(
				ROOT_BOOT_VBUF_V0,
				// XXX(qix-): Hardcoding the ID for now, bear with.
				4_294_967_297,
				idx,
				syscall::key!($field),
			)?
		}};
	}

	let vbuf_addr: u64 = 0x600_0000_0000 + idx * 0x1_0000_0000;

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
			syscall::set!(
				ROOT_BOOT_VBUF_V0,
				// XXX(qix-): Hardcoding the ID for now, bear with.
				4_294_967_297,
				idx,
				syscall::key!("!vmbase!"),
				vbuf_addr
			)?;

			vbuf_addr as *mut u8
		},
	})
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
}

// Sleeps between a frame.
//
// NOTE(qix-): Temporary function. Please do not copy into your modules.
#[doc(hidden)]
fn sleep_between_frame() {
	// TODO(qix-): Implement a proper sleep syscall.
	for _ in 0..100_000_000 {
		unsafe {
			core::arch::asm!("nop");
		}
	}
}

#[no_mangle]
extern "Rust" fn main() {
	write_str("looking for vbuf 0...\n");
	if let Ok(vbuf) = find_video_buffer(0) {
		write_str("found vbuf 0\n");

		if (vbuf.bits_per_pixel & 0b111) != 0 {
			write_str("vbuf 0 is not byte-aligned\n");
			return;
		}

		if vbuf.red_mask != 8 {
			write_str("vbuf 0 red channel is not 8 bits\n");
			return;
		}

		if vbuf.green_mask != 8 {
			write_str("vbuf 0 green channel is not 8 bits\n");
			return;
		}

		if vbuf.blue_mask != 8 {
			write_str("vbuf 0 blue channel is not 8 bits\n");
			return;
		}

		let left = (vbuf.width >> 1) - (OroLogo::WIDTH as u64 >> 1);
		let top = (vbuf.height >> 1) - (OroLogo::HEIGHT as u64 >> 1);

		let mut iter = OroLogo::new();

		let mut fade_in = 255u8;

		loop {
			let mut off = 0usize;

			#[doc(hidden)]
			static mut OFF_SCREEN: [u8; (OroLogo::WIDTH * OroLogo::HEIGHT) / 4] =
				[0; { (OroLogo::WIDTH * OroLogo::HEIGHT) / 4 }];

			fade_in = fade_in.saturating_sub(FADE_IN_STEP);

			loop {
				match iter.next() {
					None => {
						write_str("Oro logo exhausted commands (shouldn't happen)");
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
									OFF_SCREEN[byte_off] =
										OFF_SCREEN[byte_off] & !(0b11 << bit_off)
											| ((lightness & 0b11) << bit_off);
								}
							}
						} else {
							// Otherwise, we can draw directly.
							let color = (lightness & 0b11) << 6;

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
						let color = lightness << 6;
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

			sleep_between_frame(/*1000 / OroLogo::FPS as u64*/);
		}
	} else {
		write_str("failed to find vbuf 0\n");
	}
}
