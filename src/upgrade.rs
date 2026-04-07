use crate::{consts::SOCKET_UPGRADE_PATH, fuse::FuseFS};
use errno::errno;
use fuser::{Filesystem, Session};
use std::{
    fs, mem,
    os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
    ptr,
};

#[repr(C)]
struct ScmCmsgHeader {
    cmsg_len: usize,
    cmsg_level: libc::c_int,
    cmsg_type: libc::c_int,
    fd: libc::c_int,
}

pub fn start_graceful_upgrade(fs: FuseFS) -> i32 {
    println!("Upgrade socket exists, performing upgrade");
    let fuse_fd = unsafe {
        let fuse_fd = recv_fd_from_peer();
        OwnedFd::from_raw_fd(fuse_fd)
    };

    let acl = fuser::SessionACL::All;
    let mut session = Session::from_fd(fs, fuse_fd, acl);

    println!("Starting fuse session with transferred file descriptor");
    match session.run() {
        Ok(_) => {
            println!("Fuse session ended successfully");
            return 0;
        }
        Err(e) => {
            eprintln!("Fuse session ended with error: {:?}", e);
            return -1;
        },
    }
}

pub fn exit_graceful_upgrade() -> i32 {
    let sock_fd;
    let peer_fd;
    let fuse_fd;

    unsafe {
        sock_fd = create_bind_socket();
        peer_fd = listen_on_socket(sock_fd);
        fuse_fd = find_fuse_fd();
        send_fd_to_peer(peer_fd, fuse_fd);
    }

    let _ = fs::remove_file(SOCKET_UPGRADE_PATH);

    0
}

unsafe fn create_bind_socket() -> i32 {
    let sock_fd;

    // Creating a socket
    sock_fd = libc::socket(libc::AF_LOCAL, libc::SOCK_STREAM, 0);
    if sock_fd < 0 {
        panic!("Failed to create socket. Error code [{}]", sock_fd);
    }

    println!("Upgrade socket created successfully. sock_fd [{}]", sock_fd);

    // Removing the socket file if it exists
    let _ = fs::remove_file(SOCKET_UPGRADE_PATH);

    // Binding the socket, creates the socket file
    let mut addr = libc::sockaddr_un {
        sun_family: libc::AF_UNIX as u16,
        sun_path: [0; 108],
    };
    for (i, c) in SOCKET_UPGRADE_PATH.chars().enumerate() {
        addr.sun_path[i] = c as i8;
    }
    let result = libc::bind(
        sock_fd,
        &addr as *const libc::sockaddr_un as *const libc::sockaddr,
        mem::size_of::<libc::sockaddr_un>() as u32,
    );
    if result < 0 {
        panic!(
            "Failed to bind socket. Error code [{}]. Errno [{}]",
            errno().0,
            errno()
        );
    }

    println!("Upgrade socket created and bound successfully");

    sock_fd
}

unsafe fn listen_on_socket(sock_fd: i32) -> i32 {
    let result = libc::listen(sock_fd, 1);
    if result < 0 {
        panic!("Failed to listen on socket. Error code [{}]", result);
    }

    println!("Starting listening on socket");

    let peer_fd = libc::accept(sock_fd, std::ptr::null_mut(), std::ptr::null_mut());
    if peer_fd < 0 {
        panic!(
            "Failed to accept connection on socket. Error code [{}]. Errno is [{}]",
            errno().0,
            errno()
        );
    }

    println!(
        "Accepted connection on socket. Peer socket file descriptor [{}]",
        peer_fd
    );

    peer_fd
}

unsafe fn recv_fd_from_peer() -> i32 {
    let sock_fd = libc::socket(libc::AF_LOCAL, libc::SOCK_STREAM, 0);
    if sock_fd < 0 {
        panic!("Failed to create socket. Error code [{}]", sock_fd);
    }

    let mut addr = libc::sockaddr_un {
        sun_family: libc::AF_UNIX as u16,
        sun_path: [0; 108],
    };
    for (i, c) in SOCKET_UPGRADE_PATH.chars().enumerate() {
        addr.sun_path[i] = c as i8;
    }

    println!("Connecting to socket");
    let result = libc::connect(
        sock_fd,
        &addr as *const libc::sockaddr_un as *const libc::sockaddr,
        mem::size_of::<libc::sockaddr_un>() as u32,
    );
    if result < 0 {
        panic!(
            "Failed to connect to socket. Error code [{}]. Errno was [{}]",
            errno().0,
            errno()
        );
    }
    println!("Connected to addr, sock_fd [{}]", sock_fd);

    let mut scmhdr = ScmCmsgHeader {
        cmsg_len: 0,
        cmsg_level: 0,
        cmsg_type: 0,
        fd: 0,
    };

    let mut buf = [0u8; 2];
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr() as *mut libc::c_void,
        iov_len: buf.len(),
    };

    let mut mhdr = libc::msghdr {
        msg_name: ptr::null_mut(),
        msg_namelen: 0,
        msg_iov: &mut iov as *mut libc::iovec,
        msg_iovlen: 1,
        msg_control: &mut scmhdr as *mut ScmCmsgHeader as *mut libc::c_void,
        msg_controllen: mem::size_of::<ScmCmsgHeader>() as usize,
        msg_flags: 0,
    };

    println!("Receving message from peer");
    let result = libc::recvmsg(sock_fd, &mut mhdr, 0);
    if result < 0 {
        panic!("Failed to receive fd from peer. Error code [{}]", result);
    }

    println!("Received fd from peer [{}]", scmhdr.fd);

    scmhdr.fd
}

unsafe fn send_fd_to_peer(peer_fd: i32, fd: i32) {
    let scmhdr = ScmCmsgHeader {
        cmsg_len: (mem::size_of::<libc::cmsghdr>() + mem::size_of::<libc::c_int>()) as usize,
        cmsg_level: libc::SOL_SOCKET,
        cmsg_type: libc::SCM_RIGHTS,
        fd,
    };

    let mut iovec = libc::iovec {
        iov_base: ":)".as_ptr() as *mut libc::c_void,
        iov_len: 2,
    };

    let msg = libc::msghdr {
        msg_name: ptr::null_mut(),
        msg_namelen: 0,
        msg_iov: &mut iovec as *mut libc::iovec,
        msg_iovlen: 1,
        msg_control: &scmhdr as *const ScmCmsgHeader as *mut libc::c_void,
        msg_controllen: mem::size_of::<ScmCmsgHeader>() as usize,
        msg_flags: 0,
    };

    println!("Sending fd [{}] to peer [{}]", fd, peer_fd);
    let result = libc::sendmsg(peer_fd, &msg, 0);
    if result < 0 {
        panic!(
            "Failed to send fd to peer. Error code [{}]. Errno is [{}]",
            errno().0,
            errno()
        );
    }
    println!("Sent fd to peer successfully");
}

fn find_fuse_fd() -> i32 {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .into_iter()
        .find(|entry| {
            if let Ok(entry) = entry {
                let path = entry.path();
                let fd = path.file_name().unwrap().to_str().unwrap();

                return fs::read_link(format!("/proc/self/fd/{}", fd))
                    .unwrap()
                    .to_str()
                    .unwrap()
                    == "/dev/fuse";
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
