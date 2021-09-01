use syn::spanned::Spanned;

use super::*;

pub fn parse_command_attr(args: AttributeArgs) -> Result<Options> {
    let mut opt = Options::Leaf(Default::default());
    for arg in args {
        match arg {
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("subcommands") => {
                let inner = opt.to_parent()?;
                if !inner.subcommands.is_empty() {
                    return Err(Error::new(list.span(), "duplicate argument `subcommands`"));
                }
                for subcmd in list.nested {
                    match subcmd {
                        NestedMeta::Meta(Meta::Path(subcmd)) => inner.subcommands.push(subcmd),
                        NestedMeta::Lit(Lit::Str(s)) => {
                            inner.subcommands.push(syn::parse_str(&s.value())?)
                        }
                        NestedMeta::Meta(Meta::List(mut self_impl))
                            if self_impl.path.is_ident("self") =>
                        {
                            if self_impl.nested.len() == 1 {
                                match self_impl.nested.pop().unwrap().into_value() {
                                    NestedMeta::Meta(Meta::Path(self_impl)) => {
                                        if inner.self_impl.is_some() {
                                            return Err(Error::new(
                                                self_impl.span(),
                                                "duplicate argument `self`",
                                            ));
                                        }
                                        inner.self_impl = Some(SelfImplInfo {
                                            path: self_impl,
                                            is_async: false,
                                            blocking: false,
                                        })
                                    }
                                    NestedMeta::Meta(Meta::List(l)) if l.nested.len() == 1 => {
                                        if inner.self_impl.is_some() {
                                            return Err(Error::new(
                                                self_impl.span(),
                                                "duplicate argument `self`",
                                            ));
                                        }
                                        let blocking = match l.nested.first().unwrap() {
                                            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("blocking") => true,
                                            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("async") => false,
                                            arg => return Err(Error::new(arg.span(), "unknown argument")),
                                        };
                                        inner.self_impl = Some(SelfImplInfo {
                                            path: l.path,
                                            is_async: !blocking,
                                            blocking,
                                        })
                                    }
                                    a => {
                                        return Err(Error::new(
                                            a.span(),
                                            "`self` implementation must be a path, or a list with 1 argument",
                                        ))
                                    }
                                }
                            } else {
                                return Err(Error::new(
                                    self_impl.nested.span(),
                                    "`self` can only have one implementation",
                                ));
                            }
                        }
                        arg => {
                            return Err(Error::new(arg.span(), "unknown argument to `subcommands`"))
                        }
                    }
                }
                if inner.subcommands.is_empty() {
                    return Err(Error::new(
                        list.path.span(),
                        "`subcommands` requires at least 1 argument",
                    ));
                }
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("subcommands") => {
                return Err(Error::new(
                    p.span(),
                    "`subcommands` requires at least 1 argument",
                ));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("subcommands") => {
                return Err(Error::new(
                    nv.path.span(),
                    "`subcommands` cannot be assigned to",
                ));
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("display") => {
                if list.nested.len() == 1 {
                    match &list.nested[0] {
                        NestedMeta::Meta(Meta::Path(display_impl)) => {
                            if opt.common().display.is_some() {
                                return Err(Error::new(
                                    display_impl.span(),
                                    "duplicate argument `display`",
                                ));
                            }
                            opt.common().display = Some(display_impl.clone())
                        }
                        a => {
                            return Err(Error::new(
                                a.span(),
                                "`display` implementation must be a path",
                            ))
                        }
                    }
                } else {
                    return Err(Error::new(
                        list.nested.span(),
                        "`display` can only have one implementation",
                    ));
                }
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("display") => {
                return Err(Error::new(p.span(), "`display` requires an argument"));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("display") => {
                return Err(Error::new(
                    nv.path.span(),
                    "`display` cannot be assigned to",
                ));
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("custom_cli") => {
                if list.nested.len() == 1 {
                    match &opt.common().exec_ctx {
                        ExecutionContext::Standard => match &list.nested[0] {
                            NestedMeta::Meta(Meta::Path(custom_cli_impl)) => {
                                opt.common().exec_ctx = ExecutionContext::CustomCli {
                                    custom: list.path,
                                    cli: custom_cli_impl.clone(),
                                    is_async: false,
                                }
                            }
                            NestedMeta::Meta(Meta::List(custom_cli_impl)) if custom_cli_impl.nested.len() == 1 => {
                                let is_async = match custom_cli_impl.nested.first().unwrap() {
                                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("async") => false,
                                    arg => return Err(Error::new(arg.span(), "unknown argument")),
                                };
                                opt.common().exec_ctx = ExecutionContext::CustomCli {
                                    custom: list.path,
                                    cli: custom_cli_impl.path.clone(),
                                    is_async,
                                };
                            }
                            a => {
                                return Err(Error::new(
                                    a.span(),
                                    "`custom_cli` implementation must be a path, or a list with 1 argument",
                                ))
                            }
                        },
                        ExecutionContext::CliOnly(_) => {
                            return Err(Error::new(list.span(), "duplicate argument: `cli_only`"))
                        }
                        ExecutionContext::RpcOnly(_) => {
                            return Err(Error::new(
                                list.span(),
                                "`cli_only` and `rpc_only` are mutually exclusive",
                            ))
                        }
                        ExecutionContext::Local(_) => {
                            return Err(Error::new(
                                list.span(),
                                "`cli_only` and `local` are mutually exclusive",
                            ))
                        }
                        ExecutionContext::CustomCli { .. } => {
                            return Err(Error::new(
                                list.span(),
                                "`cli_only` and `custom_cli` are mutually exclusive",
                            ))
                        }
                    }
                } else {
                    return Err(Error::new(
                        list.nested.span(),
                        "`custom_cli` can only have one implementation",
                    ));
                }
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("custom_cli") => {
                return Err(Error::new(p.span(), "`custom_cli` requires an argument"));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("custom_cli") => {
                return Err(Error::new(
                    nv.path.span(),
                    "`custom_cli` cannot be assigned to",
                ));
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("aliases") => {
                if !opt.common().aliases.is_empty() {
                    return Err(Error::new(list.span(), "duplicate argument `alias`"));
                }
                for nested in list.nested {
                    match nested {
                        NestedMeta::Lit(Lit::Str(alias)) => opt.common().aliases.push(alias),
                        a => return Err(Error::new(a.span(), "`alias` must be a string")),
                    }
                }
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("alias") => {
                return Err(Error::new(p.span(), "`alias` requires an argument"));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("alias") => {
                if !opt.common().aliases.is_empty() {
                    return Err(Error::new(nv.path.span(), "duplicate argument `alias`"));
                }
                if let Lit::Str(alias) = nv.lit {
                    opt.common().aliases.push(alias);
                } else {
                    return Err(Error::new(nv.lit.span(), "`alias` must be a string"));
                }
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("cli_only") => {
                match &opt.common().exec_ctx {
                    ExecutionContext::Standard => {
                        opt.common().exec_ctx = ExecutionContext::CliOnly(p)
                    }
                    ExecutionContext::CliOnly(_) => {
                        return Err(Error::new(p.span(), "duplicate argument: `cli_only`"))
                    }
                    ExecutionContext::RpcOnly(_) => {
                        return Err(Error::new(
                            p.span(),
                            "`cli_only` and `rpc_only` are mutually exclusive",
                        ))
                    }
                    ExecutionContext::Local(_) => {
                        return Err(Error::new(
                            p.span(),
                            "`cli_only` and `local` are mutually exclusive",
                        ))
                    }
                    ExecutionContext::CustomCli { .. } => {
                        return Err(Error::new(
                            p.span(),
                            "`cli_only` and `custom_cli` are mutually exclusive",
                        ))
                    }
                }
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("cli_only") => {
                return Err(Error::new(
                    list.path.span(),
                    "`cli_only` does not take any arguments",
                ));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("cli_only") => {
                return Err(Error::new(
                    nv.path.span(),
                    "`cli_only` cannot be assigned to",
                ));
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("rpc_only") => {
                match &opt.common().exec_ctx {
                    ExecutionContext::Standard => {
                        opt.common().exec_ctx = ExecutionContext::RpcOnly(p)
                    }
                    ExecutionContext::RpcOnly(_) => {
                        return Err(Error::new(p.span(), "duplicate argument: `rpc_only`"))
                    }
                    ExecutionContext::CliOnly(_) => {
                        return Err(Error::new(
                            p.span(),
                            "`rpc_only` and `cli_only` are mutually exclusive",
                        ))
                    }
                    ExecutionContext::Local(_) => {
                        return Err(Error::new(
                            p.span(),
                            "`rpc_only` and `local` are mutually exclusive",
                        ))
                    }
                    ExecutionContext::CustomCli { .. } => {
                        return Err(Error::new(
                            p.span(),
                            "`rpc_only` and `custom_cli` are mutually exclusive",
                        ))
                    }
                }
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("rpc_only") => {
                return Err(Error::new(
                    list.path.span(),
                    "`rpc_only` does not take any arguments",
                ));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("rpc_only") => {
                return Err(Error::new(
                    nv.path.span(),
                    "`rpc_only` cannot be assigned to",
                ));
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("local") => {
                match &opt.common().exec_ctx {
                    ExecutionContext::Standard => {
                        opt.common().exec_ctx = ExecutionContext::Local(p)
                    }
                    ExecutionContext::Local(_) => {
                        return Err(Error::new(p.span(), "duplicate argument: `local`"))
                    }
                    ExecutionContext::RpcOnly(_) => {
                        return Err(Error::new(
                            p.span(),
                            "`local` and `rpc_only` are mutually exclusive",
                        ))
                    }
                    ExecutionContext::CliOnly(_) => {
                        return Err(Error::new(
                            p.span(),
                            "`local` and `cli_only` are mutually exclusive",
                        ))
                    }
                    ExecutionContext::CustomCli { .. } => {
                        return Err(Error::new(
                            p.span(),
                            "`local` and `custom_cli` are mutually exclusive",
                        ))
                    }
                }
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("local") => {
                return Err(Error::new(
                    list.path.span(),
                    "`local` does not take any arguments",
                ));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("local") => {
                return Err(Error::new(nv.path.span(), "`local` cannot be assigned to"));
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("blocking") => {
                if opt.common().blocking.is_some() {
                    return Err(Error::new(p.span(), "duplicate argument `blocking`"));
                }
                opt.common().blocking = Some(p);
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("blocking") => {
                return Err(Error::new(
                    list.path.span(),
                    "`blocking` does not take any arguments",
                ));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("blocking") => {
                return Err(Error::new(
                    nv.path.span(),
                    "`blocking` cannot be assigned to",
                ));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("about") => {
                if let Lit::Str(about) = nv.lit {
                    if opt.common().about.is_some() {
                        return Err(Error::new(about.span(), "duplicate argument `about`"));
                    }
                    opt.common().about = Some(about);
                } else {
                    return Err(Error::new(nv.lit.span(), "about message must be a string"));
                }
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("about") => {
                return Err(Error::new(
                    list.path.span(),
                    "`about` does not take any arguments",
                ));
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("about") => {
                return Err(Error::new(p.span(), "`about` must be assigned to"));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("rename") => {
                if let Lit::Str(rename) = nv.lit {
                    if opt.common().rename.is_some() {
                        return Err(Error::new(rename.span(), "duplicate argument `rename`"));
                    }
                    opt.common().rename = Some(rename);
                } else {
                    return Err(Error::new(nv.lit.span(), "`rename` must be a string"));
                }
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("rename") => {
                return Err(Error::new(
                    list.path.span(),
                    "`rename` does not take any arguments",
                ));
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("rename") => {
                return Err(Error::new(p.span(), "`rename` must be assigned to"));
            }
            NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("metadata") => {
                for meta in list.nested {
                    match meta {
                        NestedMeta::Meta(Meta::NameValue(metadata_pair)) => {
                            let ident = metadata_pair.path.get_ident().ok_or(Error::new(
                                metadata_pair.path.span(),
                                "must be an identifier",
                            ))?;
                            if opt
                                .common()
                                .metadata
                                .insert(ident.clone(), metadata_pair.lit)
                                .is_some()
                            {
                                return Err(Error::new(
                                    ident.span(),
                                    format!("duplicate metadata `{}`", ident),
                                ));
                            }
                        }
                        a => {
                            return Err(Error::new(
                                a.span(),
                                "`metadata` takes a list of identifiers to be assigned to",
                            ))
                        }
                    }
                }
            }
            NestedMeta::Meta(Meta::Path(p)) if p.is_ident("metadata") => {
                return Err(Error::new(
                    p.span(),
                    "`metadata` takes a list of identifiers to be assigned to",
                ));
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("metadata") => {
                return Err(Error::new(
                    nv.path.span(),
                    "`metadata` cannot be assigned to",
                ));
            }
            _ => {
                return Err(Error::new(arg.span(), "unknown argument"));
            }
        }
    }
    if let Options::Parent(opt) = &opt {
        if opt.self_impl.is_none() {
            if let Some(display) = &opt.common.display {
                return Err(Error::new(
                    display.span(),
                    "cannot define `display` for a command without an implementation",
                ));
            }
            match &opt.common.exec_ctx {
                ExecutionContext::CliOnly(cli_only) => {
                    return Err(Error::new(
                        cli_only.span(),
                        "cannot define `cli_only` for a command without an implementation",
                    ))
                }
                ExecutionContext::RpcOnly(rpc_only) => {
                    return Err(Error::new(
                        rpc_only.span(),
                        "cannot define `rpc_only` for a command without an implementation",
                    ))
                }
                _ => (),
            }
        }
    }
    Ok(opt)
}

pub fn parse_arg_attr(attr: Attribute, arg: PatType) -> Result<ArgOptions> {
    let arg_span = arg.span();
    let mut opt = ArgOptions {
        ty: *arg.ty,
        optional: false,
        check_is_present: false,
        help: None,
        name: match *arg.pat {
            Pat::Ident(i) => Some(i.ident),
            _ => None,
        },
        rename: None,
        short: None,
        long: None,
        parse: None,
        default: None,
        count: None,
        multiple: None,
        stdin: None,
    };
    match attr.parse_meta()? {
        Meta::List(list) => {
            for arg in list.nested {
                match arg {
                    NestedMeta::Meta(Meta::List(mut list)) if list.path.is_ident("parse") => {
                        if list.nested.len() == 1 {
                            match list.nested.pop().unwrap().into_value() {
                                NestedMeta::Meta(Meta::Path(parse_impl)) => {
                                    if opt.parse.is_some() {
                                        return Err(Error::new(
                                            list.span(),
                                            "duplicate argument `parse`",
                                        ));
                                    }
                                    opt.parse = Some(parse_impl)
                                }
                                a => {
                                    return Err(Error::new(
                                        a.span(),
                                        "`parse` implementation must be a path",
                                    ))
                                }
                            }
                        } else {
                            return Err(Error::new(
                                list.nested.span(),
                                "`parse` can only have one implementation",
                            ));
                        }
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("parse") => {
                        return Err(Error::new(p.span(), "`parse` requires an argument"));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("parse") => {
                        return Err(Error::new(nv.path.span(), "`parse` cannot be assigned to"));
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("stdin") => {
                        if opt.short.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`stdin` and `short` are mutually exclusive",
                            ));
                        }
                        if opt.long.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`stdin` and `long` are mutually exclusive",
                            ));
                        }
                        if opt.help.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`stdin` and `help` are mutually exclusive",
                            ));
                        }
                        if opt.count.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`stdin` and `count` are mutually exclusive",
                            ));
                        }
                        if opt.multiple.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`stdin` and `multiple` are mutually exclusive",
                            ));
                        }
                        opt.stdin = Some(p);
                    }
                    NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("stdin") => {
                        return Err(Error::new(
                            list.path.span(),
                            "`stdin` does not take any arguments",
                        ));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("stdin") => {
                        return Err(Error::new(nv.path.span(), "`stdin` cannot be assigned to"));
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("count") => {
                        if opt.stdin.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`stdin` and `count` are mutually exclusive",
                            ));
                        }
                        if opt.multiple.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`count` and `multiple` are mutually exclusive",
                            ));
                        }
                        opt.count = Some(p);
                    }
                    NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("count") => {
                        return Err(Error::new(
                            list.path.span(),
                            "`count` does not take any arguments",
                        ));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("count") => {
                        return Err(Error::new(nv.path.span(), "`count` cannot be assigned to"));
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("multiple") => {
                        if opt.stdin.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`stdin` and `multiple` are mutually exclusive",
                            ));
                        }
                        if opt.count.is_some() {
                            return Err(Error::new(
                                p.span(),
                                "`count` and `multiple` are mutually exclusive",
                            ));
                        }
                        opt.multiple = Some(p);
                    }
                    NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("multiple") => {
                        return Err(Error::new(
                            list.path.span(),
                            "`multiple` does not take any arguments",
                        ));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("count") => {
                        return Err(Error::new(nv.path.span(), "`count` cannot be assigned to"));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("help") => {
                        if let Lit::Str(help) = nv.lit {
                            if opt.help.is_some() {
                                return Err(Error::new(help.span(), "duplicate argument `help`"));
                            }
                            if opt.stdin.is_some() {
                                return Err(Error::new(
                                    help.span(),
                                    "`stdin` and `help` are mutually exclusive",
                                ));
                            }
                            opt.help = Some(help);
                        } else {
                            return Err(Error::new(nv.lit.span(), "help message must be a string"));
                        }
                    }
                    NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("help") => {
                        return Err(Error::new(
                            list.path.span(),
                            "`help` does not take any arguments",
                        ));
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("help") => {
                        return Err(Error::new(p.span(), "`help` must be assigned to"));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("rename") => {
                        if let Lit::Str(rename) = nv.lit {
                            if opt.rename.is_some() {
                                return Err(Error::new(
                                    rename.span(),
                                    "duplicate argument `rename`",
                                ));
                            }
                            opt.rename = Some(rename);
                        } else {
                            return Err(Error::new(nv.lit.span(), "`rename` must be a string"));
                        }
                    }
                    NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("rename") => {
                        return Err(Error::new(
                            list.path.span(),
                            "`rename` does not take any arguments",
                        ));
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("rename") => {
                        return Err(Error::new(p.span(), "`rename` must be assigned to"));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("short") => {
                        if let Lit::Str(short) = nv.lit {
                            if short.value().len() != 1 {
                                return Err(Error::new(
                                    short.span(),
                                    "`short` value must be 1 character",
                                ));
                            }
                            if opt.short.is_some() {
                                return Err(Error::new(short.span(), "duplicate argument `short`"));
                            }
                            if opt.stdin.is_some() {
                                return Err(Error::new(
                                    short.span(),
                                    "`stdin` and `short` are mutually exclusive",
                                ));
                            }
                            opt.short = Some(short);
                        } else {
                            return Err(Error::new(nv.lit.span(), "`short` must be a string"));
                        }
                    }
                    NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("short") => {
                        return Err(Error::new(
                            list.path.span(),
                            "`short` does not take any arguments",
                        ));
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("short") => {
                        return Err(Error::new(p.span(), "`short` must be assigned to"));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("long") => {
                        if let Lit::Str(long) = nv.lit {
                            if opt.long.is_some() {
                                return Err(Error::new(long.span(), "duplicate argument `long`"));
                            }
                            if opt.stdin.is_some() {
                                return Err(Error::new(
                                    long.span(),
                                    "`stdin` and `long` are mutually exclusive",
                                ));
                            }
                            opt.long = Some(long);
                        } else {
                            return Err(Error::new(nv.lit.span(), "`long` must be a string"));
                        }
                    }
                    NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("long") => {
                        return Err(Error::new(
                            list.path.span(),
                            "`long` does not take any arguments",
                        ));
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("long") => {
                        return Err(Error::new(p.span(), "`long` must be assigned to"));
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("default") => {
                        if let Lit::Str(default) = nv.lit {
                            if opt.default.is_some() {
                                return Err(Error::new(
                                    default.span(),
                                    "duplicate argument `default`",
                                ));
                            }
                            opt.default = Some(default);
                        } else {
                            return Err(Error::new(nv.lit.span(), "`default` must be a string"));
                        }
                    }
                    NestedMeta::Meta(Meta::List(list)) if list.path.is_ident("default") => {
                        return Err(Error::new(
                            list.path.span(),
                            "`default` does not take any arguments",
                        ));
                    }
                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("default") => {
                        return Err(Error::new(p.span(), "`default` must be assigned to"));
                    }
                    _ => {
                        return Err(Error::new(arg.span(), "unknown argument"));
                    }
                }
            }
        }
        Meta::Path(_) => (),
        Meta::NameValue(nv) => return Err(Error::new(nv.span(), "`arg` cannot be assigned to")),
    }
    if opt.name.is_none() {
        return Err(Error::new(
            arg_span,
            "cannot infer name for pattern argument",
        ));
    }
    Ok(opt)
}

pub fn parse_param_attrs(item: &mut ItemFn) -> Result<Vec<ParamType>> {
    let mut params = Vec::new();
    for param in item.sig.inputs.iter_mut() {
        if let FnArg::Typed(param) = param {
            let mut ty = ParamType::None;
            let mut i = 0;
            while i != param.attrs.len() {
                if param.attrs[i].path.is_ident("arg") {
                    let attr = param.attrs.remove(i);
                    if matches!(ty, ParamType::None) {
                        ty = ParamType::Arg(parse_arg_attr(attr, param.clone())?);
                    } else if matches!(ty, ParamType::Arg(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` attribute may only be specified once",
                        ));
                    } else if matches!(ty, ParamType::Context(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` and `context` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::ParentData(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` and `parent_data` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Request) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` and `request` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Response) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` and `response` are mutually exclusive",
                        ));
                    }
                } else if param.attrs[i].path.is_ident("context") {
                    let attr = param.attrs.remove(i);
                    if matches!(ty, ParamType::None) {
                        ty = ParamType::Context(*param.ty.clone());
                    } else if matches!(ty, ParamType::Context(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`context` attribute may only be specified once",
                        ));
                    } else if matches!(ty, ParamType::Arg(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` and `context` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::ParentData(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`context` and `parent_data` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Request) {
                        return Err(Error::new(
                            attr.span(),
                            "`context` and `request` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Response) {
                        return Err(Error::new(
                            attr.span(),
                            "`context` and `response` are mutually exclusive",
                        ));
                    }
                } else if param.attrs[i].path.is_ident("parent_data") {
                    let attr = param.attrs.remove(i);
                    if matches!(ty, ParamType::None) {
                        ty = ParamType::ParentData(*param.ty.clone());
                    } else if matches!(ty, ParamType::ParentData(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`parent_data` attribute may only be specified once",
                        ));
                    } else if matches!(ty, ParamType::Arg(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` and `parent_data` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Context(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`context` and `parent_data` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Request) {
                        return Err(Error::new(
                            attr.span(),
                            "`parent_data` and `request` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Response) {
                        return Err(Error::new(
                            attr.span(),
                            "`parent_data` and `response` are mutually exclusive",
                        ));
                    }
                } else if param.attrs[i].path.is_ident("request") {
                    let attr = param.attrs.remove(i);
                    if matches!(ty, ParamType::None) {
                        ty = ParamType::Request;
                    } else if matches!(ty, ParamType::Request) {
                        return Err(Error::new(
                            attr.span(),
                            "`request` attribute may only be specified once",
                        ));
                    } else if matches!(ty, ParamType::Arg(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` and `request` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Context(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`context` and `request` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::ParentData(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`parent_data` and `request` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Response) {
                        return Err(Error::new(
                            attr.span(),
                            "`request` and `response` are mutually exclusive",
                        ));
                    }
                } else if param.attrs[i].path.is_ident("response") {
                    let attr = param.attrs.remove(i);
                    if matches!(ty, ParamType::None) {
                        ty = ParamType::Response;
                    } else if matches!(ty, ParamType::Response) {
                        return Err(Error::new(
                            attr.span(),
                            "`response` attribute may only be specified once",
                        ));
                    } else if matches!(ty, ParamType::Arg(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`arg` and `response` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Context(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`context` and `response` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Context(_)) {
                        return Err(Error::new(
                            attr.span(),
                            "`parent_data` and `response` are mutually exclusive",
                        ));
                    } else if matches!(ty, ParamType::Request) {
                        return Err(Error::new(
                            attr.span(),
                            "`request` and `response` are mutually exclusive",
                        ));
                    }
                } else {
                    i += 1;
                }
            }
            if matches!(ty, ParamType::None) {
                return Err(Error::new(
                    param.span(),
                    "must specify either `arg`, `context`, `parent_data`, `request`, or `response` attributes",
                ));
            }
            params.push(ty)
        } else {
            return Err(Error::new(
                param.span(),
                "commands may not take `self` as an argument",
            ));
        }
    }
    Ok(params)
}
