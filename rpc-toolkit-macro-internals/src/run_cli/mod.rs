use syn::*;

pub struct MakeSeed {
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
    make_seed: Option<MakeSeed>,
    exit_fn: Option<Expr>,
}

pub mod build;
mod parse;
