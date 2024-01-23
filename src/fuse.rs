use crate::store::{memory_store::MemoryStore, store::Store};
use fuser::{consts::FOPEN_KEEP_CACHE, Filesystem};
use libc::ENOENT;
use std::time::{Duration, SystemTime};

const TTL: Duration = Duration::from_secs(1);

pub struct FuseFS {
    store: Box<dyn Store<Ino = u64>>,
}

impl FuseFS {
    pub fn new() -> Self {
        let store = MemoryStore::new().unwrap();

        return Self {
            store: Box::new(store),
        };
    }
}

impl Filesystem for FuseFS {
    // Files
    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        _parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let _res = self.store.delete_file(name.to_str().unwrap().to_owned());

        reply.ok();
    }

    fn write(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
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
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
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
        _mode: u32,
        _umask: u32,
        _flags: i32,
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
        let file = self
            .store
            .lookup_file(name.to_str().unwrap().to_owned(), parent);

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
        _mode: u32,
        _umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        let attr = self.store.create_dir(
            name.to_str().unwrap().to_owned(),
            parent,
            _req.uid(),
            _req.gid(),
        );

        match attr {
            Ok(attr) => reply.entry(&TTL, &attr, 0),
            Err(_) => reply.error(ENOENT),
        }
    }

    fn rmdir(
        &mut self,
        _req: &fuser::Request<'_>,
        _parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let res = self.store.delete_dir(name.to_str().unwrap().to_owned());
        match res {
            Ok(_) => reply.ok(),
            Err(e) => reply.error(e.raw_os_error().unwrap()),
        }
    }

    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        let entries = self.store.get_dir_entries(ino);

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                break;
            }
        }
        reply.ok();
    }

    // Misc
    fn getattr(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyAttr) {
        let attr = self.store.get_file_attr(ino);

        match attr {
            Some(attr) => reply.attr(&TTL, attr),
            None => reply.error(ENOENT),
        }
    }

    fn setattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        let attr = self.store.set_file_attr(ino, uid, gid, size);

        match attr {
            Some(attr) => reply.attr(&TTL, attr),
            None => reply.error(ENOENT),
        }
    }
}
