use syn::*;

pub struct RpcServerArgs {
    command: Path,
    ctx: Expr,
    parent_data: Option<Expr>,
    status_fn: Option<Expr>,
    middleware: punctuated::Punctuated<Expr, token::Comma>,
}

pub mod build;
mod parse;
