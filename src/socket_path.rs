use core::fmt::NumBuffer;
use std::ffi::OsStr;
use std::fmt;
use std::io::Error;
use std::path::{Path, PathBuf};

use nix::unistd::Uid;

fn get_uid() -> Uid {
    Uid::current()
}

fn push_local_socket_with_remote(path: &mut PathBuf, host: impl AsRef<OsStr>) {
    path.push("nvim-");
    let path = path.as_mut_os_string();

    path.push(host);
    path.push("-");

    let pid = nix::unistd::getpid().as_raw();
    let mut buf = NumBuffer::new();
    let pid = pid.format_into(&mut buf);

    path.push(pid);
    path.push(".sock");
}

#[cfg(target_os = "macos")]
fn local_socket_root_on_mac() -> PathBuf {
    let uid = get_uid().as_raw();

    let mut buf = NumBuffer::new();
    let uid = uid.format_into(&mut buf);

    Path::new("/var/run/nvim").join(uid)
}

#[cfg(target_os = "linux")]
fn local_socket_root_on_linux() -> PathBuf {
    let uid = get_uid().as_raw();

    let mut buf = NumBuffer::new();
    let uid = uid.format_into(&mut buf);

    Path::new("/run/user").join(uid)
}

#[cfg(target_os = "macos")]
pub fn local_socket_with_remote(remote_host: impl AsRef<OsStr>) -> PathBuf {
    let mut path = local_socket_root_on_mac();
    push_local_socket_with_remote(&mut path, remote_host);

    path
}

#[cfg(target_os = "linux")]
pub fn local_socket_with_remote(remote_host: impl AsRef<OsStr>) -> PathBuf {
    let mut path = local_socket_root_on_linux();
    push_local_socket_with_remote(&mut path, remote_host);

    path
}

#[cfg(target_os = "macos")]
pub fn local_socket() -> PathBuf {
    let mut path = local_socket_root_on_mac();
    path.push("nvim.sock");

    path
}

#[cfg(target_os = "linux")]
pub fn local_socket() -> PathBuf {
    let mut path = local_socket_root_on_linux();
    path.push("nvim.sock");

    path
}

pub fn remote_socket_on_linux(
    remote_uid: impl fmt::Display,
    remote_host: impl fmt::Display,
) -> impl fmt::Display {
    struct SockPath<U, H>(U, H);

    impl<U, H> fmt::Display for SockPath<U, H>
    where
        U: fmt::Display,
        H: fmt::Display,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "/run/user/{}/nvim-{}-{}.sock", self.0, self.1, self.0)
        }
    }

    SockPath(remote_uid, remote_host)
}

pub fn create_socket_dir_sync(socket: &Path) -> Result<(), Error> {
    use std::os::unix::fs::DirBuilderExt as _;

    if let Some(parent) = socket.parent()
        && !parent.exists()
    {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
    } else {
        Ok(())
    }
}

pub async fn create_socket_dir_async(socket: &Path) -> Result<(), Error> {
    if let Some(parent) = socket.parent()
        && !parent.exists()
    {
        tokio::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .await
    } else {
        Ok(())
    }
}
