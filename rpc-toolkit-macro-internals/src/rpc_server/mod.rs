use syn::*;

pub struct RpcServerArgs {
    command: Path,
    ctx: Expr,
    status_fn: Option<Expr>,
    middleware: punctuated::Punctuated<Expr, token::Comma>,
}

pub mod build;
mod parse;
