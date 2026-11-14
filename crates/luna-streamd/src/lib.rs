//! Daemon de captura e streaming nativo Luna Stream.

pub mod audio;
pub mod capabilities;
pub mod encoder;
pub mod handlers;
pub mod media_probe;
pub mod muxer;
pub mod naming;
pub mod pipeline;
pub mod recorder;
pub mod replay_buffer;
pub mod session;
pub mod token_store;

#[cfg(unix)]
pub mod hotkey;
#[cfg(unix)]
pub mod portal;
#[cfg(unix)]
pub mod server;
#[cfg(unix)]
pub mod socket;

#[cfg(windows)]
mod hotkey_windows;
#[cfg(windows)]
pub use hotkey_windows as hotkey;
#[cfg(windows)]
mod portal_windows;
#[cfg(windows)]
pub use portal_windows as portal;
#[cfg(windows)]
mod server_windows;
#[cfg(windows)]
pub use server_windows as server;
#[cfg(windows)]
mod socket_windows;
#[cfg(windows)]
pub use socket_windows as socket;
