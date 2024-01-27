use super::store::{FileInfo, Store};
use etcd_client::Client;
use fuser::{FileAttr, FileType};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{self, ErrorKind},
    sync::mpsc,
    time::SystemTime,
};

type Ino = u64;

pub struct EtcdStore {
    ino_count: Ino,
    client: Client,
    dirs: HashMap<Ino, Vec<Ino>>,
    ino_to_name: HashMap<Ino, String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct FileData {
    name: String,
    attr: FileAttr,
    parent: Option<Ino>,
    data: Vec<u8>,
}

impl Store for EtcdStore {
    type Ino = Ino;

    fn new() -> io::Result<Self> {
        // let endpoint = env::var("ETCD_ENDPOINT").expect("Expected Etcd endpoint");
        let endpoint = "localhost:2379";

        let (tx, rx) = mpsc::channel();
        tokio::spawn(async move {
            println!("Connecting to Etcd on endpoint {}", endpoint);
            let mut client = Client::connect([endpoint], None)
                .await
                .expect("Couldn't connect to server");

            let res = client.member_list().await;
            match res {
                Ok(res) => {
                    println!("Connected to Etcd, members list:");
                    res.members().iter().for_each(|m| {
                        println!("Etcd member: {:?}", m);
                    });

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
                    let str_root_dir = serde_yaml::to_string(&root_dir).unwrap();
                    let res = client.put("1", str_root_dir, None).await;
                    match res {
                        Ok(_) => {
                            let _ = tx.send(client);
                        }
                        Err(e) => {
                            panic!("Couldn't create root dir: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("Couldn't get Etcd member list: {}", e);
                }
            }
        });

        let client = rx.recv();
        match client {
            Ok(client) => {
                let mut dirs = HashMap::new();
                let mut ino_to_name = HashMap::new();

                dirs.insert(1, vec![]);
                ino_to_name.insert(1, ".".to_owned());

                return Ok(EtcdStore {
                    dirs,
                    client,
                    ino_to_name,
                    ino_count: 1,
                });
            }
            Err(_) => return Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn create_file(
        &mut self,
        name: String,
        parent: Ino,
        uid: u32,
        gid: u32,
    ) -> io::Result<fuser::FileAttr> {
        self.ino_count += 1;

        let new_ino = self.ino_count;

        let file_attr = create_attr(new_ino, uid, gid, FileType::RegularFile);
        let file_data = FileData {
            name: name.clone(),
            attr: file_attr.clone(),
            parent: Some(parent),
            data: vec![],
        };

        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let payload = serde_yaml::to_string(&file_data).unwrap();
            let res = client.put(new_ino.to_string(), payload, None).await;

            match res {
                Ok(_) => tx.send(Ok(())),
                Err(_) => tx.send(Err(())),
            }
        });

        let res = rx.recv();
        match res {
            Ok(_) => {
                self.ino_to_name.insert(new_ino, name);
                self.dirs.get_mut(&parent).unwrap().push(new_ino);
                return Ok(file_attr);
            }
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn delete_file(&mut self, name: String) -> io::Result<()> {
        let file_ino = self.ino_to_name.iter().find(|(_, n)| **n == name);

        if let Some((&file_ino, _)) = file_ino {
            let (tx, rx) = mpsc::channel();
            let mut client = self.client.clone();

            tokio::spawn(async move {
                let res = client.delete(file_ino.to_string(), None).await;

                match res {
                    Ok(_) => tx.send(Ok(())),
                    Err(_) => tx.send(Err(())),
                }
            });

            let res = rx.recv();
            match res {
                Ok(_) => {
                    self.ino_to_name.remove(&file_ino);
                    self.dirs.iter_mut().for_each(|(_, v)| {
                        let index_to_remove = v.iter().position(|&i| i == file_ino);
                        if let Some(index_to_remove) = index_to_remove {
                            v.remove(index_to_remove);
                        }
                    });
                    return Ok(());
                }
                Err(_) => Err(ErrorKind::BrokenPipe.into()),
            }
        } else {
            return Ok(());
        }
    }

    fn lookup_file(&self, name: String, parent: Ino) -> Option<(Ino, FileInfo)> {
        let file_ino = self.ino_to_name.iter().find(|(_, n)| *n == &name);
        match file_ino {
            Some((&ino, _)) => {
                let (tx, rx) = mpsc::channel();
                let mut client = self.client.clone();

                tokio::spawn(async move {
                    let res = client.get(ino.to_string(), None).await;

                    match res {
                        Ok(res) => {
                            res.kvs().iter().for_each(|kv| {
                                let ino = kv.key_str().unwrap();
                                let file_data = kv.value_str().unwrap();

                                let file_data =
                                    serde_yaml::from_str::<FileData>(file_data).unwrap();

                                let file_info = super::store::FileInfo {
                                    attr: file_data.attr,
                                    name: file_data.name,
                                    parent: Some(parent),
                                };

                                tx.send(Some((ino.parse::<Ino>().unwrap(), file_info)));
                            });
                        }
                        Err(_) => {
                            tx.send(None);
                        }
                    }
                });

                return rx.recv().unwrap();
            }
            None => None,
        }
    }

    fn read_data(&self, ino: Ino, offset: i64, size: u32) -> io::Result<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let res = client.get(ino.to_string(), None).await;

            match res {
                Ok(res) => {
                    res.kvs().iter().for_each(|kv| {
                        let value = kv.value_str().unwrap();
                        let file_data: FileData = serde_yaml::from_str(value).unwrap();

                        let data = file_data.data;
                        let data_len = data.len() as i64;

                        let start = offset as usize;
                        let end = (offset + size as i64) as usize;

                        let data = if end > data_len as usize {
                            data[start..].to_vec()
                        } else {
                            data[start..end].to_vec()
                        };

                        tx.send(Ok(data));
                    });
                }
                Err(_) => {
                    tx.send(Err(ErrorKind::NotFound.into()));
                }
            }
        });

        let res = rx.recv();
        match res {
            Ok(res) => res,
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn write_data(&mut self, ino: Ino, data: &[u8], offset: i64) -> io::Result<u32> {
        let mut is_appending = false;
        if offset > 0 {
            is_appending = true;
        }
        let data = data.to_vec();
        let len = data.len();
        let (tx, rx) = mpsc::channel::<io::Result<()>>();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let res = client.get(ino.to_string(), None).await;

            match res {
                Ok(res) => {
                    let kv = res.kvs().iter().nth(0).unwrap();
                    let str_file_data = kv.value_str().unwrap();
                    let mut file_data: FileData = serde_yaml::from_str(str_file_data).unwrap();
                    if offset > 0 {
                        file_data.data.extend(data);
                    } else {
                        file_data.data = data.to_vec();
                    }
                    file_data.attr.size = file_data.data.len() as u64;

                    let payload = serde_yaml::to_string(&file_data).unwrap();
                    let res = client.put(ino.to_string(), payload, None).await;

                    match res {
                        Ok(_) => tx.send(Ok(())),
                        Err(_) => tx.send(Err(ErrorKind::BrokenPipe.into())),
                    };
                }
                Err(_) => {
                    tx.send(Err(ErrorKind::NotFound.into()));
                }
            }
        });

        let res = rx.recv();
        match res {
            Ok(_) => Ok(len as u32),
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn open_file(&self, ino: Ino) -> Option<Ino> {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let res = client.get(ino.to_string(), None).await;

            match res {
                Ok(res) => {
                    let bytes_ino = res.kvs().last().unwrap().key();
                    let ino = String::from_utf8(bytes_ino.to_vec())
                        .unwrap()
                        .parse::<Ino>()
                        .unwrap();
                    tx.send(Some(ino));
                }
                Err(_) => {
                    tx.send(None);
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
        self.ino_count += 1;
        let new_ino = self.ino_count;

        let file_attr = create_attr(new_ino, uid, gid, FileType::Directory);
        let file_data = FileData {
            name: name.clone(),
            attr: file_attr.clone(),
            parent: Some(parent),
            data: vec![],
        };

        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let payload = serde_yaml::to_string(&file_data).unwrap();
            let res = client.put(new_ino.to_string(), payload, None).await;

            match res {
                Ok(_) => tx.send(Ok(())),
                Err(_) => tx.send(Err(())),
            }
        });

        let res = rx.recv();
        match res {
            Ok(_) => {
                self.dirs.insert(new_ino, vec![]);
                self.dirs
                    .iter_mut()
                    .find(|(&i, _)| i == parent)
                    .unwrap()
                    .1
                    .push(new_ino);
                self.ino_to_name.insert(new_ino, name);
                return Ok(file_attr);
            }
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn get_dir_entries(&self, ino: Ino) -> Vec<(u64, FileType, String)> {
        let mut entries = vec![
            (ino, FileType::Directory, ".".to_owned()),
            (ino, FileType::Directory, "..".to_owned()),
        ];
        let child_inos = self.dirs.get(&ino).unwrap().clone();

        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let mut result = vec![];
            for ino in child_inos.iter() {
                let res = client.get(ino.to_string(), None).await;

                match res {
                    Ok(res) => res.kvs().iter().for_each(|kv| {
                        let data = kv.value_str().unwrap();
                        let file_data = serde_yaml::from_str::<FileData>(data).unwrap();

                        let name = file_data.name;
                        let ino = file_data.attr.ino;
                        let file_type = file_data.attr.kind;

                        result.push((ino, file_type, name));
                    }),
                    Err(_) => {}
                }
            }

            tx.send(result);
        });

        let res = rx.recv();
        match res {
            Ok(res) => {
                entries.extend(res);
                return entries;
            }
            Err(_) => vec![],
        }
    }

    fn get_file_attr(&self, ino: Ino) -> Option<FileAttr> {
        let (tx, rx) = mpsc::channel();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let res = client.get(ino.to_string(), None).await;

            match res {
                Ok(res) => {
                    res.kvs().iter().for_each(|kv| {
                        let data = kv.value_str().unwrap();
                        let file_data = serde_yaml::from_str::<FileData>(data).unwrap();
                        let attr = file_data.attr.clone();

                        tx.send(Some(attr));
                    });
                }
                Err(_) => {
                    tx.send(None);
                }
            }
        });

        let res = rx.recv();
        match res {
            Ok(res) => res,
            Err(_) => None,
        }
    }

    fn delete_dir(&mut self, name: String) -> io::Result<()> {
        let file_ino = self.ino_to_name.iter().find(|(_, n)| **n == name);

        if let Some((&dir_ino, _)) = file_ino {
            let child_inos = self.dirs.get(&dir_ino).unwrap().clone();

            let (tx, rx) = mpsc::channel::<io::Result<()>>();
            let mut client = self.client.clone();

            tokio::spawn(async move {
                let inos_to_delete: Vec<String> = child_inos
                    .iter()
                    .chain(Some(&dir_ino))
                    .map(|i| i.to_string())
                    .collect();

                for ino in inos_to_delete.iter() {
                    client.delete(ino.to_owned(), None).await;
                }

                tx.send(Ok(()));
            });

            let res = rx.recv();
            match res {
                Ok(_) => {
                    self.ino_to_name.remove(&dir_ino);
                    self.dirs.iter_mut().for_each(|(_, v)| {
                        let index_to_remove = v.iter().position(|&i| i == dir_ino);
                        if let Some(index_to_remove) = index_to_remove {
                            v.remove(index_to_remove);
                        }
                    });
                    return Ok(());
                }
                Err(_) => Err(ErrorKind::BrokenPipe.into()),
            }
        } else {
            return Ok(());
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

        tokio::spawn(async move {
            let res = client.get(ino.to_string(), None).await;

            match res {
                Ok(res) => {
                    for kv in res.kvs().iter() {
                        let value = kv.value_str().unwrap();
                        let mut file_data: FileData = serde_yaml::from_str(value).unwrap();

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
                        let res = client.put(ino.to_string(), payload, None).await;

                        match res {
                            Ok(_) => {
                                let attr = file_data.attr.clone();
                                tx.send(Some(attr));
                            }
                            Err(_) => {
                                tx.send(None);
                            }
                        };
                    }
                }
                Err(_) => {
                    tx.send(None);
                }
            }
        });

        let res = rx.recv();
        match res {
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
