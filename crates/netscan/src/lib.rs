//! Network scanner: which hosts answer in an address range, what they offer (open ports and
//! the services usually behind them), what they are called and who made them.
//!
//! Everything is plain TCP connects, so no special privileges are needed. A host counts as
//! present when it accepts a connection, refuses one (a closed port answers with a reset) or
//! shows up in the system's neighbour table after we tried to talk to it (a host on the
//! local network that firewalls every port still answers ARP). The scan only ever connects
//! and disconnects: it sends no data.

mod range;
mod scan;
mod services;

pub use range::{local_range, parse_range, MAX_ADDRESSES};
pub use scan::{scan, Host, ScanOptions, ScanSummary, Service};
pub use services::{service_name, vendor_of, COMMON_PORTS, PROBE_PORTS};
