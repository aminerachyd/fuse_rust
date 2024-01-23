use super::store::Store;
use etcd_client::Client;
use fuser::{FileAttr, FileType};
use std::{
    env,
    io::{self, ErrorKind},
    sync::mpsc,
    time::SystemTime,
};

type Ino = u64;

pub struct EtcdStore {
    ino_count: Ino,
    client: Client,
}

struct FileData {
    attr: FileAttr,
    data: Vec<u8>,
}

impl Store for EtcdStore {
    type Ino = Ino;

    fn new() -> io::Result<Self> {
        let endpoint = env::var("ETCD_ENDPOINT").expect("Expected Etcd endpoint");

        let (tx, rx) = mpsc::channel();

        tokio::spawn(async move {
            let client = Client::connect([endpoint], None)
                .await
                .expect("Couldn't connect to server");

            let _ = tx.send(client);
        });

        return Ok(EtcdStore {
            ino_count: 0,
            client: rx.recv().unwrap(),
        });
    }

    fn create_file(
        &mut self,
        name: String,
        _parent: Ino,
        uid: u32,
        gid: u32,
    ) -> io::Result<fuser::FileAttr> {
        self.ino_count += 1;

        let new_ino = self.ino_count;

        let file_name = format!("{}#{}", new_ino, name);
        let file_attr = create_attr(new_ino, uid, gid, FileType::RegularFile);

        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let res = client.put(file_name, vec![], None).await;

            match res {
                Ok(_) => tx.send(Ok(())),
                Err(_) => tx.send(Err(())),
            }
        });

        let res = rx.recv();
        match res {
            Ok(_) => Ok(file_attr),
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }
}

fn create_attr(ino: Ino, uid: u32, gid: u32, kind: FileType) -> FileAttr {
    let mut perm = 0o644;
    if kind == FileType::Directory {
        perm = 0o755;
    }

    FileAttr {
        ino,
        kind,
        perm,
        uid,
        gid,
        size: 0,
        blocks: 0,
        atime: SystemTime::now(),
        mtime: SystemTime::now(),
        ctime: SystemTime::now(),
        crtime: SystemTime::now(),
        nlink: 1,
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}
