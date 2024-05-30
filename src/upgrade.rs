use std::{mem, ptr, os, fs, ffi::CString};

const SOCKET_UPGRADE_PATH: &str = "/tmp/fusefs_upgrade.sock";

pub fn graceful_upgrade() -> i32 {
    unsafe {
        create_bind_socket();
    }

    0
}

unsafe fn create_bind_socket() -> (i32, libc::sockaddr_un) {
    let sock_fd;

    // Creating a socket
    sock_fd = libc::socket(libc::AF_LOCAL, libc::SOCK_STREAM, 0);
    if sock_fd < 0 {
        panic!("Failed to create socket. Error code [{}]", sock_fd);
    }

    // Removing the socket file if it exists
    let _ = fs::remove_file(SOCKET_UPGRADE_PATH);

    // Binding the socket, creates the socket file
    let mut addr: libc::sockaddr_un = mem::zeroed();
    addr.sun_family = libc::AF_LOCAL as u16;
    ptr::copy_nonoverlapping(SOCKET_UPGRADE_PATH.as_ptr() as *const i8, 
                             addr.sun_path.as_mut_ptr().cast(), 
                             SOCKET_UPGRADE_PATH.len());
    let result = libc::bind(sock_fd, &addr as *const libc::sockaddr_un as *const libc::sockaddr, 
               mem::size_of::<libc::sockaddr_un>() as u32);
    if result < 0 {
        panic!("Failed to bind socket. Error code [{}]. Errno [{}]", result, *libc::__errno_location());
    }

    println!("Upgrade socket created and bound successfully");
    (sock_fd, addr)
}

unsafe fn listen_on_socket(sock_fd: i32, addr: *mut libc::sockaddr) {
    let mut result;
    let sock_size = mem::size_of::<libc::sockaddr_un>() as u32;
    result = libc::listen(sock_fd, 1);
    if result < 0 {
        panic!("Failed to listen on socket. Error code [{}]", result);
    } 

    let peer_fd = libc::accept(sock_fd, addr, sock_size as *mut u32);
}
