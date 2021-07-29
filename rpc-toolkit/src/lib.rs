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
/// `rpc_server!(command, context, status_fn)`
/// - returns: [Server](hyper::Server)
/// - `command`: path to an rpc command (with the `#[command]` attribute)
/// - `context`: The [Context] for `command`. Must implement [Clone](std::clone::Clone).
/// - `status_fn` (optional): a function that takes a JSON RPC error code (`i32`) and returns a [StatusCode](hyper::StatusCode)
///   - default: `|_| StatusCode::OK`
pub use rpc_toolkit_macro::rpc_server;
/// `run_cli!(command, app_mutator, make_ctx, exit_fn)`
/// - this function does not return
/// - `command`: path to an rpc command (with the `#[command]` attribute)
/// - `app_mutator` (optional): an expression that returns a mutated app.
///   - example: `app => app.arg(Arg::with_name("port").long("port"))`
///   - default: `app => app`
/// - `make_ctx` (optional): an expression that takes [&ArgMatches](clap::ArgMatches) and returns the [Context] used by `command`.
///   - example: `matches => matches.value_of("port")`
///   - default: `matches => matches`
/// - `exit_fn` (optional): a function that takes a JSON RPC error code (`i32`) and returns an Exit code (`i32`)
///   - default: `|code| code`
pub use rpc_toolkit_macro::run_cli;
pub use {clap, hyper, reqwest, serde, serde_json, tokio, url, yajrc};

pub use crate::context::Context;
pub use crate::metadata::Metadata;

#[cfg(feature = "cli-cookies")]
pub mod cli_helpers;
pub mod command_helpers;
mod context;
mod metadata;
pub mod rpc_server_helpers;
