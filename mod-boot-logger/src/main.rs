//! Boot time logger module for the Oro operating system.
//!
//! This module does its best to provide graphical (or at least
//! visual) logging output during the earlier boot stages of
//! the Oro operating system.
#![no_main]

use std::os::oro::{
	debug_out_v0_println as println,
	id::iface::{
		KERNEL_IFACE_QUERY_BY_TYPE_V0, KERNEL_IFACE_QUERY_TYPE_META_V0, ROOT_BOOT_VBUF_V0,
	},
	key,
	syscall::Error,
	syscall_get, syscall_set,
};

use oro_logo_rle::{Command, OroLogoData};

/// The Oro logo, aliased to a specific resolution.
type OroLogo = oro_logo_rle::OroLogo<oro_logo_rle::OroLogo256x256>;

/// How many steps to fade in per frame.
const FADE_IN_STEP: u8 = 2;

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
fn find_video_buffer(idx: u64) -> Result<Vbuf, (Error, u64)> {
	// Try to find the `ROOT_BOOT_VBUF_V0` interface.
	let boot_vbuf_iface = match syscall_get!(
		KERNEL_IFACE_QUERY_BY_TYPE_V0,
		KERNEL_IFACE_QUERY_BY_TYPE_V0,
		ROOT_BOOT_VBUF_V0,
		0
	) {
		Ok(iface) => {
			println!("found ROOT_BOOT_VBUF_V0: {iface:#X}");
			iface
		}
		Err((err, ext)) => {
			println!(
				"failed to find ROOT_BOOT_VBUF_V0: {err:?}[{:?}]",
				::oro::Key(&ext)
			);
			return Err((err, ext));
		}
	};

	#[doc(hidden)]
	macro_rules! get_vbuf_field {
		($field:literal) => {{ syscall_get!(ROOT_BOOT_VBUF_V0, boot_vbuf_iface, idx, key!($field),)? }};
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
				boot_vbuf_iface,
				idx,
				key!("!vmbase!"),
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

/// Lightness values mapped to grey RGB values.
const LIGHTNESSES: [u8; 4] = [0, 0x55, 0xAA, 0xFF];

#[no_mangle]
extern "Rust" fn main() {
	match syscall_get!(
		KERNEL_IFACE_QUERY_TYPE_META_V0,
		KERNEL_IFACE_QUERY_TYPE_META_V0,
		ROOT_BOOT_VBUF_V0,
		key!("icount")
	) {
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

		sleep_between_frame(/*1000 / OroLogo::FPS as u64*/);
	}
}
