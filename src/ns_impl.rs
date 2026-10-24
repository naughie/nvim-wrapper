use crate::local_nvim::{Stdio, async_nvim::Nvim};
use crate::socket_path;

use std::env;
use std::io::Error as IoError;
use std::ops::ControlFlow;
use std::path::Path;
use std::process::Termination;
use std::process::{ExitCode, ExitStatus};

use tokio::process::Child;
use tokio::task::JoinHandle;

use openrussh_client::connect::{Connection, Handler};

enum InheritedTermination {
    Io(IoError),
    Exit(ExitStatus),
    Other,
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
            Self::Other => ExitCode::FAILURE,
        }
    }
}

fn connect_to_nvim_server(nvim_sock: &Path) -> Result<Child, IoError> {
    Nvim::new(Stdio::Inherit)
        .args(["--remote-ui", "--server"])
        .arg(nvim_sock)
        .spawn()
}

async fn tail_wait(res: Result<Child, IoError>) -> InheritedTermination {
    match res {
        Ok(mut child) => match child.wait().await {
            Ok(status) => InheritedTermination::Exit(status),
            Err(e) => InheritedTermination::Io(e),
        },
        Err(e) => InheritedTermination::Io(e),
    }
}

async fn forward_socket<H: Handler + Send + 'static>(
    conn: Connection<H>,
    local_socket: &Path,
    remote_socket: &str,
) -> ControlFlow<(), JoinHandle<()>> {
    use openrussh_client::connect::Disconnect;
    use tokio::net::UnixListener;

    let Ok(listener) = UnixListener::bind(local_socket) else {
        return ControlFlow::Break(());
    };

    let remote_socket = remote_socket.to_owned();

    let task = tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                conn.disconnect(Disconnect::ByApplication, "forwarding done", "en")
                    .await
                    .ok();

                return;
            };

            let Ok(fwd) = conn.forward_local_uds(remote_socket.clone(), socket).await else {
                conn.disconnect(Disconnect::ByApplication, "forwarding done", "en")
                    .await
                    .ok();

                return;
            };

            if fwd.wait().await.is_err() {
                conn.disconnect(Disconnect::ByApplication, "forwarding done", "en")
                    .await
                    .ok();

                return;
            }
        }
        //
    });

    ControlFlow::Continue(task)
}

async fn start_remote_nvim(
    remote_host: &str,
    local_nvim_sock: &Path,
) -> ControlFlow<(), JoinHandle<()>> {
    use openrussh_client::config::Target;
    use openrussh_client::known_hosts::KnownHosts;
    use openrussh_client::process::Chunk;
    use openrussh_client::process::Command;
    use openrussh_client::process::{Redirect, RedirectPath};
    use openrussh_client::ssh2_config::{ParseRule, SshConfig};

    let Ok(conf) = SshConfig::parse_default_file(ParseRule::ALLOW_UNSUPPORTED_FIELDS) else {
        return ControlFlow::Break(());
    };
    let target = Target::parse(remote_host);
    let Ok(hosts) = target.query(&conf) else {
        return ControlFlow::Break(());
    };

    let Ok(known_hosts) = KnownHosts::parse_default_path() else {
        return ControlFlow::Break(());
    };

    let Ok(conn) = Connection::connect(&hosts, known_hosts).await else {
        return ControlFlow::Break(());
    };

    let remote_uid = {
        let mut buf = Vec::new();

        let cmd = Command::new().prog("id").arg("-u").complete(false);
        let Ok(mut child) = conn.exec(cmd).await else {
            return ControlFlow::Break(());
        };

        while let Some(chunk) = child.read_next().await {
            if let Chunk::Stdout(data) = chunk {
                buf.extend_from_slice(&data);
            }
        }

        if let Ok(buf) = String::from_utf8(buf) {
            buf
        } else {
            return ControlFlow::Break(());
        }
    };
    let remote_uid = remote_uid.trim_ascii_end();
    let remote_sock = socket_path::remote_socket_on_linux(remote_uid, remote_host).to_string();

    let remote_socket_exists = {
        let mut exists = false;

        let cmd = Command::new()
            .prog("ss")
            .args(["-Hlx", "src", &remote_sock])
            .complete(false);
        let Ok(mut child) = conn.exec(cmd).await else {
            return ControlFlow::Break(());
        };

        while let Some(chunk) = child.read_next().await {
            if let Chunk::Stdout(data) = chunk
                && !data.trim_ascii_start().is_empty()
            {
                exists = true;
                break;
            }
        }

        exists
    };

    let task = if remote_socket_exists {
        forward_socket(conn, local_nvim_sock, &remote_sock).await?
    } else {
        let cmd = Command::new()
            .env("NVIM_SOCK", &remote_sock)
            .prog("nohup")
            .args(["nvim", "--listen", &remote_sock, "--headless"])
            .redirect(Redirect::Stdin {
                to: RedirectPath("/dev/null"),
            })
            .redirect(Redirect::Stdout {
                append: false,
                to: RedirectPath("/dev/null"),
            })
            .redirect(Redirect::Stderr {
                append: false,
                to: RedirectPath("/dev/null"),
            })
            .complete(true);
        let Ok(child) = conn.exec(cmd).await else {
            return ControlFlow::Break(());
        };
        let status = child.wait().await;
        if !status.success() {
            return ControlFlow::Break(());
        }

        let task = forward_socket(conn, local_nvim_sock, &remote_sock).await?;

        loop {
            use std::process::Stdio;
            use std::time::Duration;
            use tokio::process::Command;

            let Ok(mut child) = Command::new("nvim")
                .args(["--remote-send", "<esc>", "--server"])
                .arg(local_nvim_sock)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
            else {
                return ControlFlow::Break(());
            };
            let Ok(status) = child.wait().await else {
                return ControlFlow::Break(());
            };

            if status.success() {
                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        task
    };

    ControlFlow::Continue(task)
}

pub async fn cli_impl() -> impl Termination {
    let mut args = env::args().skip(1);

    let Some(remote_host) = args.next() else {
        return InheritedTermination::Other;
    };

    let local_nvim_sock = socket_path::local_socket_with_remote(&remote_host);

    let ControlFlow::Continue(task) = start_remote_nvim(&remote_host, &local_nvim_sock).await
    else {
        return InheritedTermination::Other;
    };

    let child = connect_to_nvim_server(&local_nvim_sock);
    let ret = tail_wait(child).await;

    task.abort();

    ret
}
