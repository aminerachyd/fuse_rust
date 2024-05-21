use fuser::{FileAttr, FileType};
use std::io;

type Ino = u64;

#[derive(Clone)]
pub struct FileInfo {
    pub parent: Option<Ino>,
    pub name: String,
    pub attr: FileAttr,
}

// Simplified interface to provide storage for files and directories
pub trait Store: Send {
    type Ino: 'static;

    fn new() -> io::Result<Self>
    where
        Self: Sized;

    // Files
    fn delete_file(&mut self, name: String) -> io::Result<()>;
    fn write_data(&mut self, ino: Ino, data: &[u8], offset: i64) -> io::Result<u32>;
    fn open_file(&self, ino: Ino) -> Option<Ino>;
    fn read_data(&self, ino: Ino, offset: i64, size: u32) -> io::Result<Vec<u8>>;
    fn create_file(
        &mut self,
        name: String,
        parent: Ino,
        uid: u32,
        gid: u32,
    ) -> io::Result<FileAttr>;

    // Dirs
    fn lookup_file(&self, name: String, parent: Ino) -> Option<(Ino, FileInfo)>;
    fn create_dir(&mut self, name: String, parent: Ino, uid: u32, gid: u32)
        -> io::Result<FileAttr>;

    fn delete_dir(&mut self, name: String) -> io::Result<()>;
    fn get_dir_entries(&self, ino: Ino) -> Vec<(u64, FileType, String)>;

    // Misc
    fn get_file_attr(&self, ino: Ino) -> Option<FileAttr>;
    fn set_file_attr(
        &mut self,
        ino: Ino,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
    ) -> Option<FileAttr>;
}

#[derive(Debug)]
pub enum StoreType {
    InMemory,
    Etcd,
}
