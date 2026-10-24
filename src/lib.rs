#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("this crate only supports Linux and macOS");

mod socket_path;

mod local_nvim;

pub mod n_impl;

pub mod ns_impl;
