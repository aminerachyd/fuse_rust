use super::store::FileInfo;
use super::store::Store;
use fuser::FileAttr;
use fuser::FileType;
use libc::ENOTEMPTY;
use std::io;
use std::{collections::HashMap, time::SystemTime};

type Ino = <MemoryStore as Store>::Ino;

pub struct MemoryStore {
    ino_counter: Ino,
    files: HashMap<Ino, FileInfo>,
    files_data: HashMap<Ino, Vec<u8>>,
}

impl Store for MemoryStore {
    type Ino = u64;
    fn new() -> io::Result<Self> {
        let mut store = MemoryStore {
            ino_counter: 1,
            files: HashMap::new(),
            files_data: HashMap::new(),
        };

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

        store.files.insert(
            1,
            FileInfo {
                name: ".".to_owned(),
                kind: FileType::Directory,
                attr: root_dir_attr,
                parent: Some(1),
            },
        );

        return Ok(store);
    }

    fn delete_file(&mut self, name: String) -> io::Result<()> {
        let file_to_remove = self.files.iter().find(|(_, info)| info.name == name);

        if let Some((&ino, info)) = file_to_remove {
            if info.kind == FileType::RegularFile {
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

    fn open_file(&self, ino: Ino) -> Option<Ino> {
        let res = self.files.keys().find(|&i| ino == *i);
        match res {
            Some(_) => Some(ino),
            None => None,
        }
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
        let attr = create_attr(ino, uid, gid, FileType::RegularFile);

        let new_fileinfo = FileInfo {
            attr,
            name,
            parent: Some(parent),
            kind: FileType::RegularFile,
        };

        self.files.insert(ino, new_fileinfo);
        self.files_data.insert(ino, vec![]);

        Ok(attr)
    }

    // Dirs
    fn lookup_file(&self, name: String, parent: Ino) -> Option<(u64, FileInfo)> {
        let res = self.files.iter().find(|(_, info)| {
            info.parent.is_some() && info.parent.unwrap() == parent && info.name == name
        });

        match res {
            Some((&ino, info)) => Some((ino, info.clone())),
            None => None,
        }
    }

    fn create_dir(
        &mut self,
        name: String,
        parent: Ino,
        uid: u32,
        gid: u32,
    ) -> io::Result<FileAttr> {
        self.ino_counter += 1;
        let ino = self.ino_counter;
        let attr = create_attr(ino, uid, gid, FileType::Directory);
        let new_fileinfo = FileInfo {
            attr,
            name,
            parent: Some(parent),
            kind: FileType::Directory,
        };

        self.files.insert(ino, new_fileinfo);
        Ok(attr)
    }

    fn delete_dir(&mut self, name: String) -> io::Result<()> {
        let dir_to_remove = self.files.iter().find(|(_, info)| info.name == name);

        if let Some((&dir_ino, info)) = dir_to_remove {
            if info.kind == FileType::Directory {
                let not_empty = self
                    .files
                    .iter()
                    .filter(|(_, info)| info.parent.is_some() && info.parent.unwrap() == dir_ino)
                    .map(|(ino, _)| *ino)
                    .count()
                    > 0;

                if not_empty {
                    return Err(io::Error::from_raw_os_error(ENOTEMPTY));
                }

                self.files_data.remove(&dir_ino);
                self.files.remove(&dir_ino);
            }
        }

        Ok(())
    }

    fn get_dir_entries(&self, ino: Ino) -> Vec<(u64, FileType, String)> {
        let mut entries = vec![(ino, FileType::Directory, "..".to_string())];
        if ino != 1 {
            entries.push((ino, FileType::Directory, ".".to_string()));
        }

        self.files.iter().for_each(|(ino_child, info)| {
            if info.parent.is_some() && info.parent.unwrap() == ino {
                entries.push((*ino_child, info.kind, info.name.to_string()));
            }
        });

        return entries;
    }

    // Misc
    fn get_file_attr(&self, ino: Ino) -> Option<FileAttr> {
        let file = self.files.get(&ino);
        match file {
            Some(fileinfo) => Some(fileinfo.attr),
            None => None,
        }
    }

    fn set_file_attr(
        &mut self,
        ino: Ino,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
    ) -> Option<FileAttr> {
        let file = self.files.get_mut(&ino);
        match file {
            Some(fileinfo) => {
                fileinfo.attr.size = size.unwrap_or(fileinfo.attr.size);
                fileinfo.attr.uid = uid.unwrap_or(fileinfo.attr.uid);
                fileinfo.attr.gid = gid.unwrap_or(fileinfo.attr.gid);
                fileinfo.attr.atime = SystemTime::now();
                fileinfo.attr.mtime = SystemTime::now();
                fileinfo.attr.ctime = SystemTime::now();

                Some(fileinfo.attr)
            }
            None => None,
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
