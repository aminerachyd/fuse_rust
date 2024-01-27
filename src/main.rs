mod fuse;
mod store;

use fuse::FuseFS;
use fuser::MountOption;
use std::{fs, io};
use store::store::StoreType;

#[tokio::main]
async fn main() -> io::Result<()> {
    let mountpoint = "/tmp/fusefs";
    let store_type = {
        StoreType::InMemory
        // StoreType::Etcd
    };

    let file_system = FuseFS::new(&store_type);

    if let Err(_) = fs::read_dir(mountpoint) {
        fs::create_dir(mountpoint)?
    } else {
        fs::remove_dir_all(mountpoint)?;
        fs::create_dir(mountpoint)?
    }

    let opts = &[MountOption::AllowOther, MountOption::AutoUnmount];

    println!(
        "Mounting fuse filesystem on [{}] using mode [{:?}]...",
        mountpoint, store_type
    );

    fuser::mount2(file_system, mountpoint, opts)
}
