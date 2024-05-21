mod fuse;
mod store;
mod upgrade;

use fuse::FuseFS;
use fuser::MountOption;
use std::{env, fs, io, sync::mpsc, thread};
use store::store::StoreType;
use signal_hook::{consts::{SIGTERM,SIGINT}, iterator::Signals};
use upgrade::upgrade_sock;

const DEFAULT_STORE_TYPE: StoreType = StoreType::InMemory;
const DEFAULT_MOUNTPOINT: &str = "/tmp/fusefs";

#[tokio::main]
async fn main() -> io::Result<()> {
    let upgrade = env::args().into_iter().find(|arg| arg == "--upgrade").is_some();
    let store_type = get_store_from_env(DEFAULT_STORE_TYPE);
    let mountpoint = get_mountpoint_from_env(DEFAULT_MOUNTPOINT.to_string());
    let file_system = FuseFS::new(&store_type);

    let opts = &[MountOption::AllowOther, MountOption::AutoUnmount];

    println!(
        "Mounting fuse filesystem on [{}] using mode [{:?}]...",
        mountpoint, store_type
    );

    let (tx, rx) = mpsc::channel();

    let mut signals = Signals::new(&[SIGTERM, SIGINT])?;
    thread::spawn(move || {
        for sig in signals.forever() {
            graceful_exit(sig, &rx, upgrade);
        }
    });

    let _ = tx.send(fuser::mount2(file_system, mountpoint, opts));

    Ok(())
}

fn graceful_exit(sig: i32, rx: &mpsc::Receiver<io::Result<()>>, upgrade: bool) {
    match sig {
        SIGINT => {
            println!("Received SIGINT, waiting for file system to be unmounted to exit gracefully");
        }
        SIGTERM => {
            println!("Received SIGTERM, waiting for file system to be unmounted to exit gracefully");
        }
        _ => {
            println!("Unhandled signal received: [{}], ignoring", sig);
        }
    }

    let mut code = 0;

    match rx.recv() {
        Ok(Ok(_)) => {
            println!("Successfully unmmounted FUSE filesystem. Exiting");
        },
        Ok(Err(e)) => {
            eprintln!("Failed to mount fuse filesystem: {:?}", e);
            code = 1;
        },
        Err(e) => {
            eprintln!("Failed to communicate unmount result: {:?}", e);
            code = 1;
        },
    }

    std::process::exit(code);
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

    if let Err(_) = fs::read_dir(mountpoint.clone()) {
        println!("Creating mountpoint [{}]", mountpoint.clone());
        fs::create_dir(mountpoint.clone()).unwrap();
    } else {
        println!(
            "Mountpoint [{}] already exists, removing it and creating a new one",
            mountpoint.clone()
        );
        fs::remove_dir_all(mountpoint.clone()).unwrap();
        fs::create_dir(mountpoint.clone()).unwrap();
    }

    mountpoint
}
