pub use cli::*;
pub use command::*;
pub use context::Context;
/// `#[command(...)]`
/// - `#[command(cli_only)]` -> executed by CLI instead of RPC server (leaf commands only)
/// - `#[command(rpc_only)]` -> no CLI bindings (leaf commands only)
/// - `#[command(local)]` -> executed wherever it was invoked. (By RPC when hit from RPC server, by cli when invoked from CLI)
/// - `#[command(blocking)]` -> run with [spawn_blocking](tokio::task::spawn_blocking) if in an async context
/// - `#[command(about = "About text")]` -> Set about text for the command
/// - `#[command(rename = "new_name")]` -> Set the name of the command to `new_name` in the RPC and CLI
/// - `#[command(subcommands(...))]` -> Set this as the parent command for the listed subcommands
///   - note: the return type of the decorated function must be the [Context] required by its subcommands, and all args must implement [Clone](std::clone::Clone).
/// - `#[command(subcommands(self(self_command_impl)))]` -> If no subcommand is provided, call this function
///   - `self_command_impl :: Context ctx, Display res, Into<RpcError> err => ctx -> Result<res, err>`
///   - note: [Display](std::fmt::Display) is not required for `res` if it has a custom display function that will take it as input
///   - if `self_command_impl` is async, write `self(self_command_impl(async))`
///   - if `self_command_impl` is blocking, write `self(self_command_impl(blocking))`
///   - default: require a subcommand if subcommands are specified
/// - `#[command(display(custom_display_fn))]` -> Use the function `custom_display_fn` to display the command's result (leaf commands only)
///   - `custom_display_fn :: res -> ()`
///   - note: `res` is the type of the decorated command's output
///   - default: `default_display`
///
/// See also: [arg](rpc_toolkit_macro::arg), [context](rpc_toolkit_macro::context)
pub use rpc_toolkit_macro::command;
pub use {clap, futures, hyper, reqwest, serde, serde_json, tokio, url, yajrc};

mod cli;
mod command;
// pub mod command_helpers;
mod context;
// mod metadata;
// pub mod rpc_server_helpers;
mod util;
