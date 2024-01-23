mod fuse;
mod store;

use fuse::FuseFS;
use fuser::MountOption;
use std::{fs, io};

fn main() -> io::Result<()> {
    let mountpoint = "/tmp/fusefs";
    let passthrough_fs = FuseFS::new();

    if let Err(_) = fs::read_dir(mountpoint) {
        fs::create_dir(mountpoint)?
    }

    let opts = &[MountOption::AllowOther, MountOption::AutoUnmount];

    println!("Mounting {}", mountpoint);

    fuser::mount2(passthrough_fs, mountpoint, opts)
}
