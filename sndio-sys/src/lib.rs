//! Bindings for [`sndio`].
//!
//! This crate provides low-level bindings for [`sndio`] generated using
//! [`bindgen`].
//!
//! > Sndio is a small audio and MIDI framework part of the OpenBSD
//! > project and ported to FreeBSD, Linux and NetBSD. It provides a
//! > lightweight audio & MIDI server and a fully documented user-space
//! > API to access either the server or directly the hardware in a uniform
//! > way.
//!
//! See [`sndio`] documentation for more information about each API:
//!
//!  - [`sio_open`] and other `sio_*` methods for accessing the audio server
//!    for playback/recording.
//!  - [`mio_open`] and other `mio_*` methods for accessing MIDI hardware.
//!  - [`sioctl_open`] and other `sioctl_*` methods for accessing control
//!    parameters of audio devices.
//!
//! [`sndio`]: http://www.sndio.org/
//! [`bindgen`]: https://github.com/rust-lang/rust-bindgen
//! [`sio_open`]: http://man.openbsd.org/sio_open
//! [`mio_open`]: http://man.openbsd.org/mio_open
//! [`sioctl_open`]: http://man.openbsd.org/sioctl_open

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub type pollfd = libc::pollfd;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
