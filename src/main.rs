mod fuse;
mod store;
mod upgrade;
mod exit;
mod consts;

use fuse::FuseFS;
use fuser::{MountOption, Session, SessionACL};
use std::{env, fs, io::{self, ErrorKind}, sync::mpsc, thread};
use store::store::StoreType;
use exit::{graceful_exit, handle_signal};
use signal_hook::{consts::{SIGTERM, SIGINT}, iterator::Signals};
use upgrade::receive_fuse_fd;

use crate::consts::SOCKET_UPGRADE_PATH;

#[tokio::main]
async fn main() -> io::Result<()> {
    let upgrade = env::args().any(|arg| arg == "--upgrade");

    let store_type = get_store_from_env(consts::DEFAULT_STORE_TYPE);
    let mountpoint = get_mountpoint_from_env(consts::DEFAULT_MOUNTPOINT.to_string());
    let file_system = FuseFS::new(&store_type);

    // Signal handling — set up for both normal and upgrade paths so that
    // every instance can hand off the fuse fd to the next one.
    let (unmount_tx, unmount_rx) = mpsc::channel();
    let (sig_tx, sig_rx) = mpsc::channel::<i32>();

    let mut signals = Signals::new(&[SIGTERM, SIGINT])?;
    let mut sig_count = 0;
    thread::spawn(move || {
        for sig in signals.forever() {
            handle_signal(sig, &sig_tx, &mut sig_count);
        }
    });

    thread::spawn(move || {
        loop {
           graceful_exit(&unmount_rx, &sig_rx, upgrade);
        }
    });

    if upgrade && fs::metadata(SOCKET_UPGRADE_PATH).is_ok() {
        println!("Upgrade socket exists, performing upgrade");
        let fuse_fd = receive_fuse_fd();
        let mut session = Session::from_fd(file_system, fuse_fd, SessionACL::All);
        println!("Starting fuse session with transferred file descriptor");
        let _ = unmount_tx.send(session.run());
    } else {
        if upgrade {
            println!("Upgrade socket does not exist, not performing graceful upgrade");
        }

        let opts: &[MountOption] = if upgrade {
            &[MountOption::AllowOther]
        } else {
            &[MountOption::AllowOther, MountOption::AutoUnmount]
        };

        println!(
            "Mounting fuse filesystem on [{}] using mode [{:?}]...",
            mountpoint, store_type
        );

        let _ = unmount_tx.send(fuser::mount2(file_system, mountpoint, opts));
    }

    Ok(())
}


fn get_store_from_env(default: StoreType) -> StoreType {
    let store_env = env::var("FUSEFS_STORE_TYPE");

    if let Ok(str_store_type) = store_env {
        match str_store_type.as_str() {
            "etcd" => {
                println!("Proceeding with [{:?}] store", StoreType::Etcd);
                return StoreType::Etcd;
            }
            "in-mem" => {
                println!("Proceeding with [{:?}] store", StoreType::InMemory);
                return StoreType::InMemory;
            }
            _ => {
                println!(
                    "Invalid store type: {}\nProceeding with [{:?}] store",
                    str_store_type, default
                );
                return default;
            }
        }
    } else {
        println!(
            "No store type specified, proceeding with default store [{:?}]",
            default
        );
        return default;
    }
}

fn get_mountpoint_from_env(default: String) -> String {
    let mountpoint_env = env::var("FUSEFS_MOUNTPOINT");
    let mountpoint;
    if let Ok(str_mountpoint) = mountpoint_env {
        println!("Proceeding with mountpoint [{}]", str_mountpoint);

        mountpoint = str_mountpoint;
    } else {
        println!(
            "No mountpoint specified, proceeding with default mountpoint [{}]",
            default
        );
        mountpoint = default.to_string();
    }

    if let Err(e) = fs::metadata(&mountpoint) {
        match e.kind() {
            ErrorKind::NotConnected => {
                println!("Mountpoint [{}] is broken but the file exists, not removing it", &mountpoint);
            },
            _ => {
                println!("Creating mountpoint [{}]", mountpoint.clone());
                fs::create_dir(mountpoint.clone()).unwrap();
            }
        }
    }
    // } else {
    //     println!(
    //         "Mountpoint [{}] already exists, removing it and creating a new one",
    //         mountpoint.clone()
    //     );
    //     fs::remove_dir_all(mountpoint.clone()).unwrap();
    //     fs::create_dir(mountpoint.clone()).unwrap();
    // }

    mountpoint
}
