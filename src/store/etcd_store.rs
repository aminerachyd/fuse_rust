use super::store::{FileInfo, Store};
use etcd_client::{Client, GetOptions};
use fuser::{FileAttr, FileType};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, ErrorKind},
    sync::mpsc,
    time::SystemTime,
};

type Ino = u64;
const DEFAULT_ETCD_ENDPOINT: &str = "http://localhost:2379";
const INO_COUNT_KEY: &str = "meta:ino_count";

fn data_key(ino: Ino) -> String {
    format!("data:{}", ino)
}

pub struct EtcdStore {
    client: Client,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileData {
    name: String,
    attr: FileAttr,
    parent: Option<Ino>,
    data: Vec<u8>,
}

impl EtcdStore {
    fn next_ino(&self) -> Ino {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let res = client.get(INO_COUNT_KEY, None).await.unwrap();
            let current: Ino = res
                .kvs()
                .first()
                .map(|kv| kv.value_str().unwrap().parse().unwrap())
                .unwrap_or(1);
            let next = current + 1;
            client
                .put(INO_COUNT_KEY, next.to_string(), None)
                .await
                .unwrap();
            tx.send(next).unwrap();
        });

        rx.recv().unwrap()
    }

    fn get_all_entries(&self) -> Vec<(Ino, FileData)> {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let opts = GetOptions::new().with_prefix();
            let res = client.get("data:", Some(opts)).await.unwrap();
            let entries: Vec<(Ino, FileData)> = res
                .kvs()
                .iter()
                .filter_map(|kv| {
                    let key = kv.key_str().ok()?;
                    let ino: Ino = key.strip_prefix("data:")?.parse().ok()?;
                    let file_data: FileData =
                        serde_yaml::from_str(kv.value_str().ok()?).ok()?;
                    Some((ino, file_data))
                })
                .collect();
            tx.send(entries).unwrap();
        });

        rx.recv().unwrap()
    }

    fn find_by_name(&self, name: &str) -> Option<(Ino, FileData)> {
        self.get_all_entries()
            .into_iter()
            .find(|(_, fd)| fd.name == name)
    }

    fn find_by_name_and_parent(&self, name: &str, parent: Ino) -> Option<(Ino, FileData)> {
        self.get_all_entries()
            .into_iter()
            .find(|(_, fd)| fd.name == name && fd.parent == Some(parent))
    }

    fn get_children(&self, parent_ino: Ino) -> Vec<(Ino, FileData)> {
        self.get_all_entries()
            .into_iter()
            .filter(|(_, fd)| fd.parent == Some(parent_ino))
            .collect()
    }
}

impl Store for EtcdStore {
    type Ino = Ino;

    fn new() -> io::Result<Self> {
        let endpoint = get_etcd_endpoint_from_env(DEFAULT_ETCD_ENDPOINT.to_string());

        let (tx, rx) = mpsc::channel();
        tokio::spawn(async move {
            println!("Connecting to Etcd on endpoint [{}]", endpoint);
            let mut client = Client::connect([endpoint], None)
                .await
                .expect("Couldn't connect to server");

            match client.member_list().await {
                Err(e) => {
                    println!("Couldn't get Etcd member list: [{}]", e);
                }
                Ok(res) => {
                    println!("Connected to Etcd, members list:");
                    res.members().iter().for_each(|m| {
                        println!("Etcd member: [{:?}]", m);
                    });

                    // Only initialize root dir and counter if not already present
                    let counter_res = client.get(INO_COUNT_KEY, None).await.unwrap();
                    if counter_res.kvs().is_empty() {
                        client
                            .put(INO_COUNT_KEY, "1", None)
                            .await
                            .unwrap();

                        let root_dir_attr = FileAttr {
                            ino: 1,
                            size: 0,
                            blocks: 0,
                            atime: SystemTime::now(),
                            mtime: SystemTime::now(),
                            ctime: SystemTime::now(),
                            crtime: SystemTime::now(),
                            kind: FileType::Directory,
                            perm: 0o755,
                            nlink: 2,
                            uid: 0,
                            gid: 0,
                            rdev: 0,
                            flags: 0,
                            blksize: 512,
                        };
                        let root_dir = FileData {
                            name: ".".to_owned(),
                            attr: root_dir_attr,
                            parent: None,
                            data: vec![],
                        };
                        let payload = serde_yaml::to_string(&root_dir).unwrap();
                        client.put(data_key(1), payload, None).await.unwrap();
                    }

                    let _ = tx.send(client);
                }
            }
        });

        let client = rx.recv();
        match client {
            Ok(client) => Ok(EtcdStore { client }),
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn create_file(
        &mut self,
        name: String,
        parent: Ino,
        uid: u32,
        gid: u32,
    ) -> io::Result<fuser::FileAttr> {
        let new_ino = self.next_ino();

        let file_attr = create_attr(new_ino, uid, gid, FileType::RegularFile);
        let file_data = FileData {
            name,
            attr: file_attr.clone(),
            parent: Some(parent),
            data: vec![],
        };

        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();
        let key = data_key(new_ino);

        tokio::spawn(async move {
            let payload = serde_yaml::to_string(&file_data).unwrap();
            match client.put(key, payload, None).await {
                Ok(_) => tx.send(Ok(())),
                Err(_) => tx.send(Err(())),
            }
        });

        match rx.recv() {
            Ok(Ok(_)) => Ok(file_attr),
            _ => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn delete_file(&mut self, name: String) -> io::Result<()> {
        let entry = self.find_by_name(&name);

        if let Some((file_ino, _)) = entry {
            let (tx, rx) = mpsc::channel();
            let mut client = self.client.clone();
            let key = data_key(file_ino);

            tokio::spawn(async move {
                match client.delete(key, None).await {
                    Ok(_) => tx.send(Ok(())),
                    Err(_) => tx.send(Err(())),
                }
            });

            match rx.recv() {
                Ok(Ok(_)) => Ok(()),
                _ => Err(ErrorKind::BrokenPipe.into()),
            }
        } else {
            Ok(())
        }
    }

    fn lookup_file(&self, name: String, parent: Ino) -> Option<(Ino, FileInfo)> {
        self.find_by_name_and_parent(&name, parent)
            .map(|(ino, fd)| {
                (
                    ino,
                    FileInfo {
                        attr: fd.attr,
                        name: fd.name,
                        parent: Some(parent),
                    },
                )
            })
    }

    fn read_data(&self, ino: Ino, offset: i64, size: u32) -> io::Result<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();
        let key = data_key(ino);

        tokio::spawn(async move {
            match client.get(key, None).await {
                Ok(res) => {
                    if let Some(kv) = res.kvs().first() {
                        let file_data: FileData =
                            serde_yaml::from_str(kv.value_str().unwrap()).unwrap();
                        let data = file_data.data;
                        let start = offset as usize;
                        let end = (offset + size as i64) as usize;

                        let slice = if end > data.len() {
                            data[start..].to_vec()
                        } else {
                            data[start..end].to_vec()
                        };
                        tx.send(Ok(slice)).unwrap();
                    } else {
                        tx.send(Err(ErrorKind::NotFound.into())).unwrap();
                    }
                }
                Err(_) => {
                    tx.send(Err(ErrorKind::NotFound.into())).unwrap();
                }
            }
        });

        match rx.recv() {
            Ok(res) => res,
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn write_data(&mut self, ino: Ino, data: &[u8], offset: i64) -> io::Result<u32> {
        let data = data.to_vec();
        let len = data.len();
        let (tx, rx) = mpsc::channel::<io::Result<()>>();
        let mut client = self.client.clone();
        let key = data_key(ino);

        tokio::spawn(async move {
            match client.get(key.clone(), None).await {
                Ok(res) => {
                    let kv = res.kvs().first().unwrap();
                    let mut file_data: FileData =
                        serde_yaml::from_str(kv.value_str().unwrap()).unwrap();
                    if offset > 0 {
                        file_data.data.extend(data);
                    } else {
                        file_data.data = data;
                    }
                    file_data.attr.size = file_data.data.len() as u64;

                    let payload = serde_yaml::to_string(&file_data).unwrap();
                    match client.put(key, payload, None).await {
                        Ok(_) => tx.send(Ok(())).unwrap(),
                        Err(_) => tx.send(Err(ErrorKind::BrokenPipe.into())).unwrap(),
                    }
                }
                Err(_) => {
                    tx.send(Err(ErrorKind::NotFound.into())).unwrap();
                }
            }
        });

        match rx.recv() {
            Ok(Ok(_)) => Ok(len as u32),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn open_file(&self, ino: Ino) -> Option<Ino> {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();
        let key = data_key(ino);

        tokio::spawn(async move {
            match client.get(key, None).await {
                Ok(res) => {
                    if res.kvs().is_empty() {
                        tx.send(None).unwrap();
                    } else {
                        tx.send(Some(ino)).unwrap();
                    }
                }
                Err(_) => {
                    tx.send(None).unwrap();
                }
            }
        });

        rx.recv().unwrap()
    }

    fn create_dir(
        &mut self,
        name: String,
        parent: Ino,
        uid: u32,
        gid: u32,
    ) -> io::Result<FileAttr> {
        let new_ino = self.next_ino();
        let file_attr = create_attr(new_ino, uid, gid, FileType::Directory);
        let file_data = FileData {
            name,
            attr: file_attr.clone(),
            parent: Some(parent),
            data: vec![],
        };

        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();
        let key = data_key(new_ino);

        tokio::spawn(async move {
            let payload = serde_yaml::to_string(&file_data).unwrap();
            match client.put(key, payload, None).await {
                Ok(_) => tx.send(Ok(())).unwrap(),
                Err(_) => tx.send(Err(())).unwrap(),
            }
        });

        match rx.recv() {
            Ok(Ok(_)) => Ok(file_attr),
            _ => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn get_dir_entries(&self, ino: Ino) -> Vec<(u64, FileType, String)> {
        let mut entries = vec![
            (ino, FileType::Directory, ".".to_owned()),
            (ino, FileType::Directory, "..".to_owned()),
        ];

        let children = self.get_children(ino);
        for (_, fd) in children {
            entries.push((fd.attr.ino, fd.attr.kind, fd.name));
        }

        entries
    }

    fn get_file_attr(&self, ino: Ino) -> Option<FileAttr> {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();
        let key = data_key(ino);

        tokio::spawn(async move {
            match client.get(key, None).await {
                Ok(res) => {
                    if let Some(kv) = res.kvs().first() {
                        let file_data: FileData =
                            serde_yaml::from_str(kv.value_str().unwrap()).unwrap();
                        tx.send(Some(file_data.attr)).unwrap();
                    } else {
                        tx.send(None).unwrap();
                    }
                }
                Err(_) => {
                    tx.send(None).unwrap();
                }
            }
        });

        match rx.recv() {
            Ok(res) => res,
            Err(_) => None,
        }
    }

    fn delete_dir(&mut self, name: String) -> io::Result<()> {
        let entry = self.find_by_name(&name);

        if let Some((dir_ino, _)) = entry {
            let children = self.get_children(dir_ino);
            let mut inos_to_delete: Vec<Ino> = children.iter().map(|(ino, _)| *ino).collect();
            inos_to_delete.push(dir_ino);

            let (tx, rx) = mpsc::channel::<io::Result<()>>();
            let mut client = self.client.clone();

            tokio::spawn(async move {
                for ino in inos_to_delete {
                    let _ = client.delete(data_key(ino), None).await;
                }
                tx.send(Ok(())).unwrap();
            });

            match rx.recv() {
                Ok(_) => Ok(()),
                Err(_) => Err(ErrorKind::BrokenPipe.into()),
            }
        } else {
            Ok(())
        }
    }

    fn set_file_attr(
        &mut self,
        ino: Ino,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
    ) -> Option<FileAttr> {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();
        let key = data_key(ino);

        tokio::spawn(async move {
            match client.get(key.clone(), None).await {
                Ok(res) => {
                    if let Some(kv) = res.kvs().first() {
                        let mut file_data: FileData =
                            serde_yaml::from_str(kv.value_str().unwrap()).unwrap();

                        if let Some(uid) = uid {
                            file_data.attr.uid = uid;
                        }
                        if let Some(gid) = gid {
                            file_data.attr.gid = gid;
                        }
                        if let Some(size) = size {
                            file_data.attr.size = size;
                        }

                        let payload = serde_yaml::to_string(&file_data).unwrap();
                        match client.put(key, payload, None).await {
                            Ok(_) => tx.send(Some(file_data.attr)).unwrap(),
                            Err(_) => tx.send(None).unwrap(),
                        }
                    } else {
                        tx.send(None).unwrap();
                    }
                }
                Err(_) => {
                    tx.send(None).unwrap();
                }
            }
        });

        match rx.recv() {
            Ok(res) => res,
            Err(_) => None,
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

fn get_etcd_endpoint_from_env(default: String) -> String {
    let endpoint_env = std::env::var("FUSEFS_ETCD_ENDPOINT");

    if let Ok(endpoint) = endpoint_env {
        println!("Proceeding with Etcd endpoint [{}]", endpoint);
        return endpoint;
    } else {
        println!(
            "No Etcd endpoint specified, proceeding with default endpoint [{}]",
            default
        );
        return default;
    }
}
