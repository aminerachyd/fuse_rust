use crate::upgrade::exit_graceful_upgrade;
use signal_hook::consts::{SIGTERM, SIGINT};
use crate::consts::SOCKET_UPGRADE_PATH;
use std::{io, sync::mpsc, fs};

const MAX_SIG_COUNT: u8 = 1;

pub fn handle_signal(sig: i32, sig_tx: &mpsc::Sender<i32>, sig_count: &mut u8) {
    *sig_count += 1;    
    if *sig_count > MAX_SIG_COUNT {
        println!("Received multiple signals, exiting immediately");
        let _ = fs::remove_file(SOCKET_UPGRADE_PATH);
        std::process::exit(1);
    }

    match sig {
        SIGTERM => {
            println!("Received SIGTERM signal, will wait for file system to unmount safely");
        },
        SIGINT => {
            println!("Received SIGINT signal, will wait for file system to unmount safely");
        },
        _ => {
            println!("Unexpected signal: [{}], ignoring", sig);
            return;
        },
    }

    sig_tx.send(sig).unwrap();
}

pub fn graceful_exit(unmount_rx: &mpsc::Receiver<io::Result<()>>, sig_rx: &mpsc::Receiver<i32>, upgrade: bool) {
    loop {
        // Looping until we receive an exit signal
        if let Ok(_) = sig_rx.try_recv() {
            if upgrade == true {
                let exit_code = exit_graceful_upgrade();
                // std::process::exit(exit_code);
            }

            let exit_code = wait_for_fs_unmount(unmount_rx);
            std::process::exit(exit_code);
        }
    }
}

fn wait_for_fs_unmount(rx: &mpsc::Receiver<io::Result<()>>) -> i32 {
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

   code
}
