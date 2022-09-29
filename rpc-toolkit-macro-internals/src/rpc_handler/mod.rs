use syn::*;

pub struct RpcHandlerArgs {
    pub(crate) command: Path,
    pub(crate) ctx: Expr,
    pub(crate) parent_data: Option<Expr>,
    pub(crate) status_fn: Option<Expr>,
    pub(crate) middleware: punctuated::Punctuated<Expr, token::Comma>,
}

pub mod build;
mod parse;
