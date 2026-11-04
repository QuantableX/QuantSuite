//! QuantSuite's server gateway (PLAN-SERVER-CLIENT §4, §6.4). Tauri-free.
//!
//! - [`identity`]: the Ed25519 keys of the server and of every device, and the
//!   self-signed certificates made from them.
//! - [`tls`]: TLS 1.3 with both keys — the client pins the server's, the server
//!   admits only paired devices.
//! - [`pairing`] and [`words`]: the one-time code and the four words both
//!   screens show.
//! - [`registry`]: paired devices and the audit log (`gateway.db`).
//! - [`authz`]: roles against the remote manifest; [`stepup`]: the server PIN.
//! - [`server`] and [`client`]: the two ends of a connection, WebSocket frames
//!   from [`protocol`].
//! - [`net`]: the addresses the gateway may listen on.

pub mod authz;
pub mod client;
pub mod identity;
pub mod net;
pub mod pairing;
pub mod protocol;
pub mod registry;
pub mod server;
pub mod stepup;
pub mod tls;
pub mod words;

pub use authz::{DeviceRole, Manifest};
pub use identity::Identity;
pub use protocol::PROTOCOL;

/// The port the gateway listens on unless the user picks another.
pub const DEFAULT_PORT: u16 = 47100;
