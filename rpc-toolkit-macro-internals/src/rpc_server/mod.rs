use syn::*;

pub struct RpcServerArgs {
    command: Path,
    ctx: Expr,
    status_fn: Option<Expr>,
}

pub mod build;
mod parse;
