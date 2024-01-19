mod fuse;
mod store;

use crate::fuse::FuseFS;
use fuser::MountOption;
use std::{fs, io};

fn main() -> io::Result<()> {
    // TODO Make this configurable via variable
    let mountpoint = "/tmp/fusefs";
    let passthrough_fs = FuseFS::new(mountpoint.to_owned());

    if let Err(_) = fs::read_dir(mountpoint) {
        fs::create_dir(mountpoint)?
    }

    let opts = &[MountOption::AllowOther, MountOption::AutoUnmount];

    println!("Mounting {}", mountpoint);

    fuser::mount2(passthrough_fs, mountpoint, opts)
}
