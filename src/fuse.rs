use fuser::FileType as fuser_FileType;
use fuser::{consts::FOPEN_KEEP_CACHE, FileAttr, Filesystem};
use libc::{ENOENT, ENOTEMPTY};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::store::memory_store::MemoryStore;
use crate::store::store::Store;

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
    store: Box<dyn Store>,
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

impl FuseFS {
    pub fn new(root_path: String) -> Self {
        let mut fs = Self {
            root_path: root_path.clone(),
            store: Box::new(MemoryStore::new()),
            ino_counter: 1,
            files: HashMap::new(),
            files_data: HashMap::new(),
        };

        fs.files.insert(
            1,
            FileInfo {
                name: ".".to_owned(),
                kind: fuser_FileType::Directory,
                attr: ROOT_DIR_ATTR,
                parent: Some(1),
            },
        );

        return fs;
    }
}

impl Filesystem for FuseFS {
    // Files
    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let res = self.store.delete_file(name.to_str().unwrap().to_owned());

        reply.ok();
    }

    fn write(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        write_flags: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        let written = self.store.write_data(ino, data, offset).unwrap();

        reply.written(written);
    }

    fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        let ino = self.store.open_file(ino);

        match ino {
            Some(ino) => {
                let flags = FOPEN_KEEP_CACHE;
                reply.opened(*ino, flags);
            }
            None => {
                reply.error(ENOENT);
            }
        }
    }

    fn read(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        let data = self.store.read_data(ino, offset, size).unwrap();

        reply.data(&data);
    }

    fn create(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        let attr = self
            .store
            .create_file(
                name.to_str().unwrap().to_owned(),
                parent,
                _req.uid(),
                _req.gid(),
            )
            .unwrap();

        reply.created(&TTL, &attr, 0, 0, 0);
    }

    // Dirs
    fn lookup(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        let file = self.files.iter().find(|(_, info)| {
            info.parent.is_some()
                && info.parent.unwrap() == parent
                && info.name == name.to_str().unwrap()
        });

        match file {
            Some((_, info)) => {
                reply.entry(&TTL, &info.attr, 0);
            }
            None => {
                reply.error(ENOENT);
            }
        }
    }

    fn mkdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        self.ino_counter += 1;
        let ino = self.ino_counter;
        let attr = create_attr(ino, _req.uid(), _req.gid(), fuser_FileType::Directory);
        let new_fileinfo = FileInfo {
            attr,
            parent: Some(parent),
            kind: fuser_FileType::Directory,
            name: name.to_str().unwrap().to_owned(),
        };

        self.files.insert(ino, new_fileinfo);

        reply.entry(&TTL, &attr, 0);
    }

    fn rmdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let dir_to_remove = self
            .files
            .iter()
            .find(|(_, info)| info.name == name.to_str().unwrap());

        if let Some((&dir_ino, info)) = dir_to_remove {
            if info.kind == fuser_FileType::Directory {
                let not_empty = self
                    .files
                    .iter()
                    .filter(|(_, info)| info.parent.is_some() && info.parent.unwrap() == dir_ino)
                    .map(|(ino, _)| *ino)
                    .count()
                    > 0;

                if not_empty {
                    reply.error(ENOTEMPTY);
                    return;
                }

                self.files_data.remove(&dir_ino);
                self.files.remove(&dir_ino);
            }
        }

        reply.ok();
    }

    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        let mut entries = vec![(ino, fuser_FileType::Directory, "..")];
        if ino != 1 {
            entries.push((ino, fuser_FileType::Directory, "."));
        }

        self.files.iter().for_each(|(ino_child, info)| {
            if info.parent.is_some() && info.parent.unwrap() == ino {
                entries.push((*ino_child, info.kind, info.name.as_str()));
            }
        });

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                break;
            }
        }
        reply.ok();
    }

    // Misc
    fn getattr(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyAttr) {
        let fileinfo = self.files.get(&ino).unwrap();

        reply.attr(&TTL, &fileinfo.attr);
    }

    fn setattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        let fileinfo = self.files.get_mut(&ino).unwrap();

        fileinfo.attr.size = size.unwrap_or(fileinfo.attr.size);
        fileinfo.attr.uid = uid.unwrap_or(fileinfo.attr.uid);
        fileinfo.attr.gid = gid.unwrap_or(fileinfo.attr.gid);
        fileinfo.attr.atime = SystemTime::now();
        fileinfo.attr.mtime = SystemTime::now();
        fileinfo.attr.ctime = SystemTime::now();

        reply.attr(&TTL, &fileinfo.attr);
    }
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
