use std::{fs, io};

use fuser::MountOption;

use crate::passthrough::PassthroughFS;

mod passthrough;

fn main() -> io::Result<()> {
    let mountpoint = "/tmp/fusefs";
    let passthrough_fs = PassthroughFS::new(mountpoint.to_owned());

    if let Err(_) = fs::read_dir(mountpoint) {
        fs::create_dir(mountpoint)?
    }

    let opts = &[MountOption::AllowOther, MountOption::AutoUnmount];

    fuser::mount2(passthrough_fs, mountpoint, opts)
}
