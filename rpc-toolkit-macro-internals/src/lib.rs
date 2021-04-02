macro_rules! macro_try {
    ($x:expr) => {
        match $x {
            Ok(a) => a,
            Err(e) => return e.to_compile_error(),
        }
    };
}

mod command;
mod rpc_server;
mod run_cli;

pub use command::build::build as build_command;
pub use rpc_server::build::build as build_rpc_server;
pub use rpc_server::RpcServerArgs;
pub use run_cli::build::build as build_run_cli;
pub use run_cli::RunCliArgs;