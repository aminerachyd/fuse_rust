use std::{mem, ptr, fs};
use crate::consts::SOCKET_UPGRADE_PATH;

struct ScmCmsgHeader {
    cmsg_len: usize,
    cmsg_level: i32,
    cmsg_type: i32,
    fd: i32,
}

pub fn start_graceful_upgrade() -> i32 {
    if fs::metadata(SOCKET_UPGRADE_PATH).is_ok() {
        println!("Upgrade socket exists, performing upgrade");
        unsafe {
            // TODO continue here
            let fuse_fd = recv_fd_from_peer();
        }
    }

    0
}

pub fn exit_graceful_upgrade() -> i32 {
    unsafe {
        let (sock_fd, mut addr) = create_bind_socket();
        let peer_fd = listen_on_socket(sock_fd, &mut addr as *mut libc::sockaddr_un as *mut libc::sockaddr);
        let fuse_fd = find_fuse_fd();
        send_fd_to_peer(peer_fd, fuse_fd);
    }

    let _ = fs::remove_file(SOCKET_UPGRADE_PATH);

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

unsafe fn listen_on_socket(sock_fd: i32, addr: *mut libc::sockaddr) -> i32 {
    let result;
    let sock_size = mem::size_of::<libc::sockaddr_un>() as u32;
    result = libc::listen(sock_fd, 1);
    if result < 0 {
        panic!("Failed to listen on socket. Error code [{}]", result);
    } 

    println!("Starting listening on socket");

    libc::accept(sock_fd, addr, sock_size as *mut u32)
}

fn find_fuse_fd() -> i32 {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .into_iter()
        .find(|entry| {
            if let Ok(entry) = entry {
                let path = entry.path();
                let fd = path.file_name().unwrap().to_str().unwrap();

                return fs::read_link(format!("/proc/self/fd/{}", fd)).unwrap().to_str().unwrap() == "/dev/fuse";
            }

            false
        })
        .unwrap()
        .unwrap()
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i32>()
        .unwrap()
}

unsafe fn recv_fd_from_peer() -> i32 {
    let sock_fd = libc::socket(libc::AF_LOCAL, libc::SOCK_STREAM, 0);
    if sock_fd < 0 {
        panic!("Failed to create socket. Error code [{}]", sock_fd);
    }

    let mut addr: libc::sockaddr_un = mem::zeroed();
    addr.sun_family = libc::AF_LOCAL as u16;
    ptr::copy_nonoverlapping(SOCKET_UPGRADE_PATH.as_ptr() as *const i8, 
                             addr.sun_path.as_mut_ptr().cast(), 
                             SOCKET_UPGRADE_PATH.len());

    libc::connect(sock_fd, &addr as *const libc::sockaddr_un as *const libc::sockaddr,
                  mem::size_of::<libc::sockaddr_un>() as u32);

    0
}

unsafe fn send_fd_to_peer(peer_fd: i32, fd: i32) {
    let scmhdr = ScmCmsgHeader {
        cmsg_len: mem::size_of::<libc::cmsghdr>() as usize,
        cmsg_level: libc::SOL_SOCKET,
        cmsg_type: libc::SCM_RIGHTS,
        fd,
    };

    let msg = libc::msghdr {
        msg_name: ptr::null_mut(),
        msg_namelen: 0,
        msg_iov: ptr::null_mut(),
        msg_iovlen: 0,
        msg_control: &scmhdr as *const ScmCmsgHeader as *mut libc::c_void,
        msg_controllen: mem::size_of::<ScmCmsgHeader>() as usize,
        msg_flags: 0,
    };

    let result = libc::sendmsg(peer_fd, &msg, 0);
    if result < 0 {
        panic!("Failed to send fd to peer. Error code [{}]", result);
    }
}
