use syn::*;

pub struct RpcServerArgs {
    command: Path,
    seed: Expr,
    status_fn: Option<Expr>,
}

pub mod build;
mod parse;
