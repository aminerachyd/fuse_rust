use std::io;

use fuser::FileAttr;

type Ino = u64;
pub trait Store {
    fn new() -> Self
    where
        Self: Sized;

    // Files
    fn delete_file(&mut self, name: String) -> io::Result<()>;
    fn write_data(&mut self, ino: Ino, data: &[u8], offset: i64) -> io::Result<u32>;
    fn open_file(&self, ino: Ino) -> Option<&Ino>;
    fn read_data(&self, ino: Ino, offset: i64, size: u32) -> io::Result<Vec<u8>>;
    fn create_file(
        &mut self,
        name: String,
        parent: Ino,
        uid: u32,
        gid: u32,
    ) -> io::Result<FileAttr>;

    // // Dirs
    // fn dir_info(&self, name: String, parent: Ino) -> Option<FileInfo>;
}
