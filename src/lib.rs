//! Den is a read-only terminal inspector for your developer machine. Install the `den`
//! command with `cargo install den-tui`; see the
//! [README](https://github.com/fran0x/den#readme) for what it shows and how to use it.
//!
//! **This library is internal.** It exists so the `den` binary and its integration tests
//! can share code. It has no stable API: any module may change or disappear in any
//! release, including patch releases, so do not depend on it.

#[doc(hidden)]
pub mod app;
#[doc(hidden)]
pub mod areas;
#[doc(hidden)]
pub mod inventory;
#[doc(hidden)]
pub mod listing;
#[doc(hidden)]
pub mod source;
#[doc(hidden)]
pub mod status;
#[doc(hidden)]
pub mod tui;
mod worker;
