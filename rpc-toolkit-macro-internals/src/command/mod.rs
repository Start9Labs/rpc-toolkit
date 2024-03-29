use std::collections::HashMap;

use syn::*;

pub mod build;
mod parse;

pub enum ExecutionContext {
    Standard,
    CliOnly(Path),
    RpcOnly(Path),
    Local(Path),
    CustomCli {
        custom: Path,
        cli: Path,
        context: Type,
        is_async: bool,
    },
}
impl Default for ExecutionContext {
    fn default() -> Self {
        ExecutionContext::Standard
    }
}

#[derive(Default)]
pub struct LeafOptions {
    macro_debug: bool,
    blocking: Option<Path>,
    is_async: bool,
    aliases: Vec<LitStr>,
    about: Option<LitStr>,
    rename: Option<LitStr>,
    exec_ctx: ExecutionContext,
    display: Option<Path>,
    metadata: HashMap<Ident, Lit>,
}

pub struct SelfImplInfo {
    path: Path,
    context: Type,
    is_async: bool,
    blocking: bool,
}
pub struct ParentOptions {
    common: LeafOptions,
    subcommands: Vec<Path>,
    self_impl: Option<SelfImplInfo>,
}
impl From<LeafOptions> for ParentOptions {
    fn from(opt: LeafOptions) -> Self {
        ParentOptions {
            common: opt,
            subcommands: Default::default(),
            self_impl: Default::default(),
        }
    }
}

pub enum Options {
    Leaf(LeafOptions),
    Parent(ParentOptions),
}
impl Options {
    fn to_parent(&mut self) -> Result<&mut ParentOptions> {
        if let Options::Leaf(opt) = self {
            *self = Options::Parent(std::mem::replace(opt, Default::default()).into());
        }
        Ok(match self {
            Options::Parent(a) => a,
            _ => unreachable!(),
        })
    }
    fn common(&mut self) -> &mut LeafOptions {
        match self {
            Options::Leaf(ref mut opt) => opt,
            Options::Parent(opt) => &mut opt.common,
        }
    }
}

pub struct ArgOptions {
    ty: Type,
    optional: bool,
    check_is_present: bool,
    help: Option<LitStr>,
    name: Option<Ident>,
    rename: Option<LitStr>,
    short: Option<LitChar>,
    long: Option<LitStr>,
    parse: Option<Path>,
    default: Option<Option<LitStr>>,
    count: Option<Path>,
    multiple: Option<Path>,
    stdin: Option<Path>,
}

pub enum ParamType {
    None,
    Arg(ArgOptions),
    Context(Type),
    ParentData(Type),
    Request,
    Response,
}
