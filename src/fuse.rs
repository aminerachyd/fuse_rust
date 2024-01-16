use fuser::FileType as fuser_FileType;
use fuser::{consts::FOPEN_KEEP_CACHE, FileAttr, Filesystem};
use libc::ENOENT;
use std::{
    collections::HashMap,
    fs::Metadata,
    os::unix::fs::{MetadataExt, PermissionsExt},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TTL: Duration = Duration::from_secs(1);
const DIR_ATTR_INO1: FileAttr = FileAttr {
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
    ino_counter: u64,
    files: HashMap<Ino, FileInfo>,
    files_data: HashMap<Ino, Vec<u8>>,
}

type Ino = u64;

struct FileInfo {
    parent: Option<Ino>,
    path: String,
    kind: fuser_FileType,
    attr: FileAttr,
}

impl FuseFS {
    pub fn new(root_path: String) -> Self {
        let mut fs = Self {
            root_path: root_path.clone(),
            ino_counter: 1,
            files: HashMap::new(),
            files_data: HashMap::new(),
        };

        fs.files.insert(
            1,
            FileInfo {
                path: ".".to_owned(),
                kind: fuser_FileType::Directory,
                attr: DIR_ATTR_INO1,
                parent: None,
            },
        );

        return fs;
    }
}

impl Filesystem for FuseFS {
    // Files
    fn mknod(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: fuser::ReplyEntry,
    ) {
        dbg!("MKNOD");
    }

    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        dbg!("UNLINK");
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
        let is_append = offset > 0;
        dbg!("WRITE", is_append);

        let filedata = self.files_data.get_mut(&ino).unwrap();
        if is_append {
            filedata.extend_from_slice(data);
        } else {
            filedata.clone_from(&data.to_vec());
        }

        let fileinfo = self.files.get_mut(&ino).unwrap();
        fileinfo.attr.size = filedata.len() as u64;

        reply.written(data.len() as u32);
    }

    fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        dbg!("OPEN", ino);

        let ino = self.files.keys().find(|&i| ino == *i);

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
        dbg!("READ");
        let filedata = self.files_data.get(&ino).unwrap();

        let start = offset as usize;
        let end = if start + size as usize > filedata.len() {
            filedata.len()
        } else {
            start + size as usize
        };

        let data = &filedata[start..end];

        reply.data(data);
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
        dbg!("CREATE", parent, name);
        self.ino_counter += 1;

        let ino = self.ino_counter;
        let attr = create_attr(ino, _req.uid(), _req.gid(), fuser_FileType::RegularFile);

        let new_fileinfo = FileInfo {
            attr,
            parent: Some(parent),
            kind: fuser_FileType::RegularFile,
            path: name.to_str().unwrap().to_owned(),
        };

        self.files.insert(ino, new_fileinfo);
        self.files_data.insert(ino, vec![]);

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
        dbg!("LOOKUP", parent, name);

        let file = self.files.iter().find(|(_, info)| {
            info.parent.is_some()
                && info.parent.unwrap() == parent
                && info.path == name.to_str().unwrap()
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
        dbg!("MKDIR", parent, name);

        self.ino_counter += 1;
        let ino = self.ino_counter;
        let attr = create_attr(ino, _req.uid(), _req.gid(), fuser_FileType::Directory);
        let new_fileinfo = FileInfo {
            attr,
            parent: Some(parent),
            kind: fuser_FileType::Directory,
            path: name.to_str().unwrap().to_owned(),
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
        dbg!("RMDIR");
    }

    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        dbg!("READDIR", ino);
        let mut entries = vec![(ino, fuser_FileType::Directory, "..")];

        self.files.iter().for_each(|(ino_child, info)| {
            if info.parent.is_some() && info.parent.unwrap() == ino || ino == 1 {
                entries.push((*ino_child, info.kind, info.path.as_str()));
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
        // dbg!("GETATTR", ino);

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
        dbg!("SETATTR", ino);

        let mut fileinfo = self.files.get_mut(&ino).unwrap();

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
        size: 0,
        blocks: 0,
        atime: SystemTime::now(),
        mtime: SystemTime::now(),
        ctime: SystemTime::now(),
        crtime: SystemTime::now(),
        kind,
        perm,
        nlink: 1,
        uid,
        gid,
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}
