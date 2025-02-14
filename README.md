<div align="center">
	<img src="https://raw.githubusercontent.com/oro-os/kernel/master/asset/oro-banner.svg" />
	<br>
	<h1 align="center"><b>Oro Operating System</b></h1>
	<br>
	Official distribution repository for the <strong>Oro Operating System</strong>,<br>
	a general-purpose, minimal, and novel microkernel operating system written in Rust.
	<br>
	&laquo;&nbsp;<a href="https://oro.sh">oro.sh</a>&nbsp;|&nbsp;<a href="https://discord.gg/WXavRNqcDS">discord</a>&nbsp;|&nbsp;<a href="https://x.com/oro_sys">x</a>&nbsp;&raquo;
	<h1></h1>
	<br>
	<br>
</div>

The **Oro Operating System**, a novel micro-kernel operating system
built from the ground up.

This repository houses all official distribution modules for various
flavors of the Oro operating system, as well as package scripts and
other distribution utilities.

> [!CAUTION]
> The Oro Operating System is currently in the early stages of development.
> It is not yet suitable for use in a production environment.

## Building

To build the Oro Operating System modules, you will need to have the interim
Rust toolchain installed. You can install the latest build by visiting
the [`oro-os/toolchain` actions](https://github.com/oro-os/toolchain/actions)
list and downloading the build artifact from the latest successful run.

Unpack that artifact into some directory, then link the toolchain (the build
infrastructure defaults to `+oro-dev`):

```sh
cd /path/to/toolchain
ls # should have bin/, lib/, etc.
rustup toolchain link oro-dev .
```

Then, build the Oro Operating System modules:

```sh
cargo +oro-dev build --release
```

Emitted modules will be in `target/x86_64-unknown-oro-elf/{debug,release}/*.oro` and can
be included in kernel configurations, etc.

## Security
If you have found a vulnerability within the Oro kernel or any of the associated
crates included in this repository, **please do not open an issue** and instead
consult [SECURITY.md](SECURITY.md) for instructions on how to responsibly disclose
your findings.

# License
The Oro Operating System is &copy; 2016-2025 by Joshua Lee Junon,
and licensed under the [Mozilla Public License 2.0](LICENSE).
