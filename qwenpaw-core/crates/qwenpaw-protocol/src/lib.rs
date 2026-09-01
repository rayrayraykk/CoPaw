mod contract;
mod events;
mod rpc;
mod types;

pub use contract::*;
pub use events::*;
pub use rpc::*;
pub use types::*;

pub const PROTOCOL_VERSION: u32 = 3;

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
