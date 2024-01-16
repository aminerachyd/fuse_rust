use std::{
    collections::HashMap,
    fs::{self, File, FileType, Metadata},
    os::unix::fs::{MetadataExt, PermissionsExt},
    time::{Duration, SystemTime},
};

use fuser::FileType as fuser_FileType;
use fuser::{consts::FOPEN_KEEP_CACHE, FileAttr, Filesystem};

const TTL: Duration = Duration::from_secs(10);

pub struct PassthroughFS {
    pub root_path: String,
    ino_to_path: HashMap<u64, FileInfo>,
}

struct FileInfo {
    ino: u64,
    path: String,
    kind: fuser_FileType,
}

impl PassthroughFS {
    pub fn new(root_path: String) -> Self {
        let mut fs = Self {
            root_path: root_path.clone(),
            ino_to_path: HashMap::new(),
        };

        fs.ino_to_path.insert(
            1,
            FileInfo {
                ino: 1,
                path: root_path,
                kind: fuser_FileType::Directory,
            },
        );

        return fs;
    }
}

impl Filesystem for PassthroughFS {
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
        let path = format!("{}/{}", self.root_path, name.to_str().unwrap());
        let f = File::create(&path).unwrap();

        let md = f.metadata().unwrap();
        let ino = md.ino();

        let fileinfo = FileInfo {
            ino,
            path,
            kind: fuser_FileType::RegularFile,
        };

        self.ino_to_path.insert(ino, fileinfo);

        let attr = fileattr_from_md(md);
        let flags = FOPEN_KEEP_CACHE;
        let ttl = &TTL;
        let generation = 0;
        let fh = 0;
        reply.created(ttl, &attr, generation, fh, flags);
    }

    // fn getattr(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyAttr) {
    //     let fileinfo = self.ino_to_path.get(&ino).unwrap();

    //     match fileinfo.kind {
    //         fuser_FileType::RegularFile => {
    //             let f = File::open(&fileinfo.path).unwrap();
    //             let md = f.metadata().unwrap();
    //             let attr = fileattr_from_md(md);
    //             reply.attr(&TTL, &attr);
    //         }
    //         fuser_FileType::Directory => {
    //             let md = fs::metadata(&fileinfo.path).unwrap();
    //             dbg!(&md);
    //             let attr = fileattr_from_md(md);
    //             reply.attr(&TTL, &attr);
    //         }
    //         _ => {}
    //     }
    // }

    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        let fileinfo = self.ino_to_path.get(&ino).unwrap();

        match fileinfo.kind {
            fuser_FileType::Directory => {
                let paths = fs::read_dir(&fileinfo.path).unwrap();

                paths.for_each(|path| {
                    let path = path.unwrap().path();
                    let md = path.metadata().unwrap();
                    let ino = md.ino();
                    let kind = if md.is_dir() {
                        fuser_FileType::Directory
                    } else {
                        fuser_FileType::RegularFile
                    };

                    let name = path.file_name().unwrap().to_str().unwrap();
                    dbg!(name);

                    reply.add(ino, offset, kind, name);
                });

                reply.ok();
            }
            _ => {
                dbg!("Not a dir");
            }
        }
    }

    fn opendir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _flags: i32,
        reply: fuser::ReplyOpen,
    ) {
        let fileinfo = self.ino_to_path.get(&ino).unwrap();

        match fileinfo.kind {
            fuser_FileType::Directory => {
                let flags = FOPEN_KEEP_CACHE;
                reply.opened(0, flags);
            }
            _ => {
                dbg!("Not a dir");
            }
        }
    }
}

fn i64_to_systemtime(t: i64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(t as u64)
}

fn fileattr_from_md(md: Metadata) -> FileAttr {
    FileAttr {
        ino: md.ino(),
        size: md.size() as u64,
        blksize: md.blksize() as u32,
        blocks: md.blocks() as u64,
        crtime: i64_to_systemtime(md.ctime()),
        atime: i64_to_systemtime(md.atime()),
        ctime: i64_to_systemtime(md.ctime()),
        uid: md.uid(),
        gid: md.gid(),
        kind: fuser::FileType::RegularFile,
        mtime: i64_to_systemtime(md.mtime()),
        nlink: md.nlink() as u32,
        perm: md.permissions().mode() as u16,
        rdev: md.rdev() as u32,
        flags: 0,
    }
}
