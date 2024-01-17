use fuser::FileType as fuser_FileType;
use fuser::{consts::FOPEN_KEEP_CACHE, FileAttr, Filesystem};
use libc::{ENOENT, ENOTEMPTY};
use std::io;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::store::Store;

pub struct MemoryStore {
    ino_counter: Ino,
    files: HashMap<Ino, FileInfo>,
    files_data: HashMap<Ino, Vec<u8>>,
}

impl Store for MemoryStore {
    fn new() -> Self {
        Self {
            ino_counter: 1,
            files: HashMap::new(),
            files_data: HashMap::new(),
        }
    }

    fn delete_file(&mut self, name: String) -> io::Result<()> {
        let file_to_remove = self.files.iter().find(|(_, info)| info.name == name);

        if let Some((&ino, info)) = file_to_remove {
            if info.kind == fuser_FileType::RegularFile {
                self.files_data.remove(&ino);
                self.files.remove(&ino);
            }
        }

        Ok(())
    }

    fn write_data(&mut self, ino: Ino, data: &[u8], offset: i64) -> io::Result<u32> {
        let is_append = offset > 0;

        let filedata = self.files_data.get_mut(&ino).unwrap();
        if is_append {
            filedata.extend_from_slice(data);
        } else {
            filedata.clone_from(&data.to_vec());
        }

        let fileinfo = self.files.get_mut(&ino).unwrap();
        fileinfo.attr.size = filedata.len() as u64;

        Ok(data.len() as u32)
    }

    fn open_file(&self, ino: Ino) -> Option<&Ino> {
        self.files.keys().find(|&i| ino == *i)
    }

    fn read_data(&self, ino: Ino, offset: i64, size: u32) -> io::Result<Vec<u8>> {
        let filedata = self.files_data.get(&ino).unwrap();

        let start = offset as usize;
        let end = if start + size as usize > filedata.len() {
            filedata.len()
        } else {
            start + size as usize
        };

        Ok(filedata[start..end].to_vec())
    }

    fn create_file(
        &mut self,
        name: String,
        parent: Ino,
        uid: u32,
        gid: u32,
    ) -> io::Result<FileAttr> {
        self.ino_counter += 1;

        let ino = self.ino_counter;
        let attr = create_attr(ino, uid, gid, fuser_FileType::RegularFile);

        let new_fileinfo = FileInfo {
            attr,
            name,
            parent: Some(parent),
            kind: fuser_FileType::RegularFile,
        };

        self.files.insert(ino, new_fileinfo);
        self.files_data.insert(ino, vec![]);

        Ok(attr)
    }
}

// #### TO REFACTOR

const TTL: Duration = Duration::from_secs(1);
const ROOT_DIR_ATTR: FileAttr = FileAttr {
    ino: 1,
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH,
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    crtime: UNIX_EPOCH,
    kind: fuser_FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 0,
    gid: 0,
    rdev: 0,
    flags: 0,
    blksize: 512,
};

pub struct FuseFS {
    pub root_path: String,
    ino_counter: Ino,
    files: HashMap<Ino, FileInfo>,
    files_data: HashMap<Ino, Vec<u8>>,
}

type Ino = u64;

#[derive(Debug)]
struct FileInfo {
    parent: Option<Ino>,
    name: String,
    kind: fuser_FileType,
    attr: FileAttr,
}

fn create_attr(ino: Ino, uid: u32, gid: u32, kind: fuser_FileType) -> FileAttr {
    let mut perm = 0o644;
    if kind == fuser_FileType::Directory {
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
