use std::process::Termination;

#[tokio::main]
async fn main() -> impl Termination {
    let ctrl_c = tokio::signal::ctrl_c();

    tokio::select! {
        res = n::ns_impl::cli_impl() => {
            res.report()
        }
        _ = ctrl_c => {
            ().report()
        }
    }
}
