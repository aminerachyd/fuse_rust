use crate::store::store::StoreType;

pub const DEFAULT_STORE_TYPE: StoreType = StoreType::InMemory;
pub const DEFAULT_MOUNTPOINT: &str = "/tmp/fusefs";
pub const SOCKET_UPGRADE_PATH: &str = "/tmp/fusefs_upgrade.sock";
