use signal_hook::consts::{SIGTERM, SIGINT};
use std::{io, sync::mpsc};

const MAX_SIG_COUNT: u8 = 1;

pub fn handle_signal(sig: i32, sig_tx: &mpsc::Sender<i32>, sig_count: &mut u8) {
    *sig_count += 1;    
    if *sig_count > MAX_SIG_COUNT {
        println!("Received multiple signals, exiting immediately");
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
            let mut code = 0;
        
            match unmount_rx.recv() {
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
    }
}
