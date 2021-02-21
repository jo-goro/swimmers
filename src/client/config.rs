use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::ops::Range;
use std::time::Duration;

use tokio::runtime::Runtime;

use super::EventHandler;

pub trait Configs {
	fn loopback() -> Self;

	fn lan() -> Self;

	fn wan() -> Self;
}

#[derive(Debug, Clone)]
pub struct JoinConfig {
	pub max_rounds: Option<NonZeroUsize>,
	pub seed_addrs: Box<[SocketAddr]>,
}

#[derive(Debug, Clone)]
pub struct SyncConfig {
	pub scheduler: SyncSchedulerConfig,
	pub connection: SyncConnectionConfig,
}

#[derive(Debug, Clone)]
pub struct SyncSchedulerConfig {
	pub base_interval: Duration,
	pub scale: NonZeroU32,
}

#[derive(Debug, Clone)]
pub struct SyncConnectionConfig {
	pub connect_timeout: Duration,
	pub read_timeout: Duration,
	pub write_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct BroadcastConfig {
	pub multiplier: NonZeroU32,
	pub free_bytes: usize,
}
#[derive(Debug, Clone)]
pub struct SuspicionConfig {
	pub alpha: f64,
	pub beta: f64,
	pub k: NonZeroU32,
}

#[derive(Debug, Clone)]
pub struct PingConfig {
	pub scheduler: PingSchedulerConfig,
	pub indirect_checks: Option<NonZeroUsize>,
}

#[derive(Debug, Clone)]
pub struct PingSchedulerConfig {
	pub base_interval: Duration,
	pub base_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct GossipConfig {
	pub base_interval: Duration,
	pub node_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct NodeConfig {
	pub bind_addr: SocketAddr,
	pub advertise_addr: SocketAddr,

	pub state: StateConfig,
}

#[derive(Debug, Clone)]
pub struct StateConfig {
	pub incarnation: u64,
	pub metadata: Option<Box<[u8]>>,
}

#[derive(Debug, Clone)]
pub struct IOConfig {
	pub out_buffer_size: u16,
	pub in_buffer_size: u16,

	pub suspect_dead: bool,
}

#[derive(Debug, Clone)]
pub struct AwarenessConfig {
	max: NonZeroU32,
}

#[derive(Debug, Clone)]
pub struct ReclaimConfig {
	dead: Duration,
	left: Duration,
}

#[derive(Debug, Clone)]
pub struct Config<'a, E>
where
	E: EventHandler,
{
	pub runtime: Option<&'a Runtime>,
	pub event_handler: E,
	pub awareness: AwarenessConfig,
	pub join: JoinConfig,
	pub broadcast: BroadcastConfig,
	pub suspicion: SuspicionConfig,
	pub gossip: GossipConfig,
	pub node: NodeConfig,
	pub io: IOConfig,
	pub reclaim: ReclaimConfig,
}
