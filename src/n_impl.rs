use crate::config::{NvimPath, get_local_config};
use crate::local_nvim::{Stdio, sync_nvim::Nvim};
use crate::socket_path;

use std::env;
use std::ffi::{OsStr, OsString};
use std::io::Error as IoError;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::process::Termination;
use std::process::{ExitCode, ExitStatus};

enum InheritedTermination {
    Io(IoError),
    Exit(ExitStatus),
}

impl Termination for InheritedTermination {
    fn report(self) -> ExitCode {
        use std::os::unix::process::ExitStatusExt as _;

        match self {
            Self::Io(e) => <Result<(), _> as Termination>::report(Err(e)),
            Self::Exit(status) => {
                if let Some(code) = status.code() {
                    ExitCode::from(code as u8)
                } else if let Some(signal) = status.signal() {
                    ExitCode::from((128 + signal) as u8)
                } else {
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn start_nvim_server<T: AsRef<OsStr>>(
    nvim_path: &NvimPath,
    nvim_sock: &Path,
    files: impl IntoIterator<Item = T>,
) -> Result<Child, IoError> {
    Nvim::new(Stdio::Inherit, nvim_path)
        .args(["-O3", "--listen"])
        .arg(nvim_sock)
        .args(files)
        .env("NVIM_SOCK", nvim_sock)
        .spawn()
}

fn connect_to_nvim_server(nvim_path: &NvimPath, nvim_sock: &Path) -> Result<Child, IoError> {
    Nvim::new(Stdio::Inherit, nvim_path)
        .args(["--remote-ui", "--server"])
        .arg(nvim_sock)
        .spawn()
}

fn wait(res: Result<Child, IoError>) -> Result<(), InheritedTermination> {
    let mut child = res.map_err(InheritedTermination::Io)?;
    let status = child.wait().map_err(InheritedTermination::Io)?;
    if status.success() {
        Ok(())
    } else {
        Err(InheritedTermination::Exit(status))
    }
}

fn tail_wait(res: Result<Child, IoError>) -> InheritedTermination {
    match res {
        Ok(mut child) => match child.wait() {
            Ok(status) => InheritedTermination::Exit(status),
            Err(e) => InheritedTermination::Io(e),
        },
        Err(e) => InheritedTermination::Io(e),
    }
}

fn open_file(nvim_path: &NvimPath, nvim_sock: &Path, files: [Option<OsString>; 3]) {
    fn open_file_impl(
        nvim_path: &NvimPath,
        nvim_sock: PathBuf,
        files: [Option<OsString>; 3],
        cwd: Option<PathBuf>,
    ) {
        if let Some(cwd) = cwd {
            let child = Nvim::new(Stdio::Discard, nvim_path)
                .arg("--server")
                .arg(&nvim_sock)
                .arg("--remote-expr")
                .arg(format!("v:lua.naughie_open_dir(\"{}\")", cwd.display()))
                .spawn();
            if wait(child).is_err() {
                return;
            }
        }

        for file in files.iter().flatten() {
            let child = Nvim::new(Stdio::Discard, nvim_path)
                .arg("--server")
                .arg(&nvim_sock)
                .arg("--remote-expr")
                .arg(format!("v:lua.naughie_open_file(\"{}\")", file.display()))
                .spawn();
            if wait(child).is_err() {
                return;
            }
        }
    }

    let cwd = env::current_dir().ok();

    let nvim_path = nvim_path.clone();
    let nvim_sock = nvim_sock.to_path_buf();

    std::thread::spawn(move || {
        open_file_impl(&nvim_path, nvim_sock, files, cwd);
    });
}

fn is_socket(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt as _;

    path.symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_socket())
}

pub fn cli_impl() -> impl Termination {
    let config = get_local_config(None);

    let nvim_sock = socket_path::local_socket();

    if is_socket(&nvim_sock) {
        let files: [Option<OsString>; 3] = {
            let mut files = env::args_os().skip(1);

            let mut buf = [None, None, None];

            buf[0] = files.next();
            buf[1] = files.next();
            buf[2] = files.next();

            buf
        };

        open_file(&config.nvim, &nvim_sock, files);

        let child = connect_to_nvim_server(&config.nvim, &nvim_sock);
        tail_wait(child)
    } else {
        let child = start_nvim_server(&config.nvim, &nvim_sock, env::args_os().skip(1));
        tail_wait(child)
    }
}
