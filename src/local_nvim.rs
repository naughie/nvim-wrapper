#[derive(Clone, Copy)]
pub enum Stdio {
    Inherit,
    Discard,
}

pub mod sync_nvim {
    use super::Stdio;

    use std::ffi::OsStr;
    use std::io::Result;
    use std::process::{Child, Command};

    pub struct Nvim {
        cmd: Command,
    }

    impl Nvim {
        pub fn new(io: Stdio) -> Self {
            let mut cmd = Command::new("nvim");

            match io {
                Stdio::Inherit => {
                    use std::process::Stdio;

                    cmd.stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit());
                }
                Stdio::Discard => {
                    use std::process::Stdio;

                    cmd.stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());
                }
            }

            Self { cmd }
        }

        pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
            self.cmd.arg(arg);
            self
        }

        pub fn args<T: AsRef<OsStr>>(&mut self, args: impl IntoIterator<Item = T>) -> &mut Self {
            self.cmd.args(args);
            self
        }

        pub fn env(&mut self, key: &str, value: impl AsRef<OsStr>) -> &mut Self {
            self.cmd.env(key, value);
            self
        }

        pub fn spawn(&mut self) -> Result<Child> {
            self.cmd.spawn()
        }
    }
}

pub mod async_nvim {
    use super::Stdio;

    use std::ffi::OsStr;
    use std::io::Result;
    use tokio::process::{Child, Command};

    pub struct Nvim {
        cmd: Command,
    }

    impl Nvim {
        pub fn new(io: Stdio) -> Self {
            let mut cmd = Command::new("nvim");

            cmd.kill_on_drop(true);

            match io {
                Stdio::Inherit => {
                    use std::process::Stdio;

                    cmd.stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit());
                }
                Stdio::Discard => {
                    use std::process::Stdio;

                    cmd.stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());
                }
            }

            Self { cmd }
        }

        pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
            self.cmd.arg(arg);
            self
        }

        pub fn args<T: AsRef<OsStr>>(&mut self, args: impl IntoIterator<Item = T>) -> &mut Self {
            self.cmd.args(args);
            self
        }

        pub fn spawn(&mut self) -> Result<Child> {
            self.cmd.spawn()
        }
    }
}
