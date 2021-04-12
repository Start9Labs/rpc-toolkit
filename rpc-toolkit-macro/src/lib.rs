use proc_macro::{Span, TokenStream};
use rpc_toolkit_macro_internals::*;

#[proc_macro_attribute]
pub fn command(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);
    let item = syn::parse_macro_input!(item as syn::ItemFn);
    build_command(args, item).into()
}

/// `#[arg(...)]` -> Take this argument as a parameter
/// - `#[arg(help = "Help text")]` -> Set help text for the arg
/// - `#[arg(rename = "new_name")]` -> Set the name of the arg to `new_name` in the RPC and CLI
/// - `#[arg(short = "a")]` -> Set the "short" representation of the arg to `-a` on the CLI
/// - `#[arg(long = "arg")]` -> Set the "long" representation of the arg to `--arg` on the CLI
/// - `#[arg(parse(custom_parse_fn))]` -> Use the function `custom_parse_fn` to parse the arg from the CLI
///   - `custom_parse_fn :: Into<RpcError> err => (&str, &ArgMatches<'_>) -> Result<arg, err>`
///   - note: `arg` is the type of the argument
/// - `#[arg(stdin)]` -> Parse the argument from stdin when using the CLI
///   - `custom_parse_fn :: Into<RpcError> err => (&[u8], &ArgMatches<'_>) -> Result<arg, err>`
/// - `#[arg(count)]` -> Treat argument as flag, count occurrences
/// - `#[arg(multiple)]` -> Allow the arg to be specified multiple times. Collect the args after parsing.
#[proc_macro_attribute]
pub fn arg(_: TokenStream, _: TokenStream) -> TokenStream {
    syn::Error::new(
        Span::call_site().into(),
        "`arg` is only allowed on arguments of a function with the `command` attribute",
    )
    .to_compile_error()
    .into()
}

/// - `#[context]` -> Passes the application context into this parameter
#[proc_macro_attribute]
pub fn context(_: TokenStream, _: TokenStream) -> TokenStream {
    syn::Error::new(
        Span::call_site().into(),
        "`context` is only allowed on arguments of a function with the `command` attribute",
    )
    .to_compile_error()
    .into()
}

#[proc_macro]
pub fn rpc_server(item: TokenStream) -> TokenStream {
    let item = syn::parse_macro_input!(item as RpcServerArgs);
    build_rpc_server(item).into()
}

#[proc_macro]
pub fn run_cli(item: TokenStream) -> TokenStream {
    let item = syn::parse_macro_input!(item as RunCliArgs);
    build_run_cli(item).into()
}
