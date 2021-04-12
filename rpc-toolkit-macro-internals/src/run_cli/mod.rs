use syn::*;

pub struct MakeCtx {
    matches_ident: Ident,
    body: Expr,
}

pub struct MutApp {
    app_ident: Ident,
    body: Expr,
}

pub struct RunCliArgs {
    command: Path,
    mut_app: Option<MutApp>,
    make_ctx: Option<MakeCtx>,
    exit_fn: Option<Expr>,
}

pub mod build;
mod parse;
