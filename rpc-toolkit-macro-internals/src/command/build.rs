use std::collections::HashSet;

use proc_macro2::*;
use quote::*;
use syn::fold::Fold;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::{Comma, Where};

use super::parse::*;
use super::*;

fn build_app(name: LitStr, opt: &mut Options, params: &mut [ParamType]) -> TokenStream {
    let about = opt.common().about.clone().into_iter();
    let (subcommand, subcommand_required) = if let Options::Parent(opt) = opt {
        (
            opt.subcommands
                .iter()
                .map(|subcmd| {
                    let mut path = subcmd.clone();
                    path.segments.last_mut().unwrap().arguments = PathArguments::None;
                    path
                })
                .collect(),
            opt.self_impl.is_none(),
        )
    } else {
        (Vec::new(), false)
    };
    let arg = params
        .iter_mut()
        .filter_map(|param| {
            if let ParamType::Arg(arg) = param {
                if arg.stdin.is_some() {
                    return None;
                }
                let name = arg.name.clone().unwrap();
                let name_str = LitStr::new(&name.to_string(), name.span());
                let help = arg.help.clone().into_iter();
                let short = arg.short.clone().into_iter();
                let long = arg.long.clone().into_iter();
                let mut modifications = TokenStream::default();
                let ty_span = arg.ty.span();
                if let Type::Path(p) = &mut arg.ty {
                    if p.path.is_ident("bool")
                        && arg.parse.is_none()
                        && (arg.short.is_some() || arg.long.is_some())
                    {
                        arg.check_is_present = true;
                        modifications.extend(quote_spanned! { ty_span =>
                            arg = arg.takes_value(false);
                        });
                    } else if arg.count.is_some() {
                        modifications.extend(quote_spanned! { ty_span =>
                            arg = arg.takes_value(false);
                            arg = arg.multiple(true);
                        });
                    } else {
                        modifications.extend(quote_spanned! { ty_span =>
                            arg = arg.takes_value(true);
                        });
                        if p.path.segments.last().unwrap().ident == "Option" {
                            arg.optional = true;
                            modifications.extend(quote_spanned! { ty_span =>
                                arg = arg.required(false);
                            });
                        } else if arg.multiple.is_some() {
                            modifications.extend(quote_spanned! { ty_span =>
                                arg = arg.multiple(true);
                            });
                        } else {
                            modifications.extend(quote_spanned! { ty_span =>
                                arg = arg.required(true);
                            });
                        }
                    }
                };
                Some(quote! {
                    {
                        let mut arg = rpc_toolkit_prelude::Arg::with_name(#name_str);
                        #(
                            arg = arg.help(#help);
                        )*
                        #(
                            arg = arg.short(#short);
                        )*
                        #(
                            arg = arg.long(#long);
                        )*
                        #modifications

                        arg
                    }
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let required = LitBool::new(subcommand_required, Span::call_site());
    let alias = &opt.common().aliases;
    quote! {
        pub fn build_app() -> rpc_toolkit_prelude::App<'static, 'static> {
            let mut app = rpc_toolkit_prelude::App::new(#name);
            #(
                app = app.about(#about);
            )*
            #(
                app = app.alias(#alias);
            )*
            #(
                app = app.arg(#arg);
            )*
            #(
                app = app.subcommand(#subcommand::build_app());
            )*
            if #required {
                app = app.setting(rpc_toolkit_prelude::AppSettings::SubcommandRequired);
            }
            app
        }
    }
}

struct GenericFilter<'a> {
    src: &'a Generics,
    lifetimes: HashSet<Lifetime>,
    types: HashSet<Ident>,
}
impl<'a> GenericFilter<'a> {
    fn new(src: &'a Generics) -> Self {
        GenericFilter {
            src,
            lifetimes: HashSet::new(),
            types: HashSet::new(),
        }
    }
    fn finish(self) -> Generics {
        let mut params: Punctuated<GenericParam, Comma> = Default::default();
        let mut where_clause = self
            .src
            .where_clause
            .as_ref()
            .map(|wc| WhereClause {
                where_token: wc.where_token,
                predicates: Default::default(),
            })
            .unwrap_or_else(|| WhereClause {
                where_token: Where(Span::call_site()),
                predicates: Default::default(),
            });
        for src_param in &self.src.params {
            match src_param {
                GenericParam::Lifetime(l) if self.lifetimes.contains(&l.lifetime) => {
                    params.push(src_param.clone())
                }
                GenericParam::Type(t) if self.types.contains(&t.ident) => {
                    params.push(src_param.clone())
                }
                _ => (),
            }
        }
        for src_predicate in self.src.where_clause.iter().flat_map(|wc| &wc.predicates) {
            match src_predicate {
                WherePredicate::Lifetime(l) if self.lifetimes.contains(&l.lifetime) => {
                    where_clause.predicates.push(src_predicate.clone())
                }
                WherePredicate::Type(PredicateType {
                    bounded_ty: Type::Path(t),
                    ..
                }) if self.types.contains(&t.path.segments.last().unwrap().ident) => {
                    where_clause.predicates.push(src_predicate.clone())
                }
                _ => (),
            }
        }
        Generics {
            lt_token: if params.is_empty() {
                None
            } else {
                self.src.lt_token.clone()
            },
            gt_token: if params.is_empty() {
                None
            } else {
                self.src.gt_token.clone()
            },
            params,
            where_clause: if where_clause.predicates.is_empty() {
                None
            } else {
                Some(where_clause)
            },
        }
    }
}
impl<'a> Fold for GenericFilter<'a> {
    fn fold_lifetime(&mut self, i: Lifetime) -> Lifetime {
        self.lifetimes
            .extend(self.src.params.iter().filter_map(|param| match param {
                GenericParam::Lifetime(l) if l.lifetime == i => Some(l.lifetime.clone()),
                _ => None,
            }));
        i
    }
    fn fold_type(&mut self, i: Type) -> Type {
        self.types.extend(
            self.src
                .params
                .iter()
                .filter_map(|param| match (param, &i) {
                    (GenericParam::Type(t), Type::Path(i)) if i.path.is_ident(&t.ident) => {
                        Some(t.ident.clone())
                    }
                    _ => None,
                }),
        );
        i
    }
}

fn rpc_handler(
    fn_name: &Ident,
    fn_generics: &Generics,
    opt: &Options,
    params: &[ParamType],
) -> TokenStream {
    let mut param_def = Vec::new();
    let mut ctx_ty = quote! { () };
    for param in params {
        match param {
            ParamType::Arg(arg) => {
                let name = arg.name.clone().unwrap();
                let rename = LitStr::new(&name.to_string(), name.span());
                let field_name = Ident::new(&format!("arg_{}", name), name.span());
                let ty = arg.ty.clone();
                param_def.push(quote! {
                    #[serde(rename = #rename)]
                    #field_name: #ty,
                })
            }
            ParamType::Context(ctx) => {
                ctx_ty = quote! { #ctx };
            }
            _ => (),
        }
    }
    let (_, fn_type_generics, _) = fn_generics.split_for_impl();
    let fn_turbofish = fn_type_generics.as_turbofish();
    let fn_path: Path = macro_try!(syn::parse2(quote! { super::#fn_name#fn_turbofish }));
    let mut param_generics_filter = GenericFilter::new(fn_generics);
    for param in params {
        if let ParamType::Arg(a) = param {
            param_generics_filter.fold_type(a.ty.clone());
        }
    }
    let param_generics = param_generics_filter.finish();
    let (_, param_ty_generics, _) = param_generics.split_for_impl();
    let param_struct_def = quote! {
        #[derive(rpc_toolkit_prelude::Deserialize)]
        pub struct Params#param_ty_generics {
            #(
                #param_def
            )*
            #[serde(flatten)]
            #[serde(default)]
            rest: rpc_toolkit_prelude::Value,
        }
    };
    let param = params.iter().map(|param| match param {
        ParamType::Arg(arg) => {
            let name = arg.name.clone().unwrap();
            let field_name = Ident::new(&format!("arg_{}", name), name.span());
            quote! { args.#field_name }
        }
        ParamType::Context(_) => quote! { ctx },
        _ => unreachable!(),
    });
    match opt {
        Options::Leaf(opt) if matches!(opt.exec_ctx, ExecutionContext::CliOnly(_)) => quote! {
            #param_struct_def

            pub async fn rpc_handler#fn_generics(
                _ctx: #ctx_ty,
                method: &str,
                _args: Params#param_ty_generics,
            ) -> Result<rpc_toolkit_prelude::Value, rpc_toolkit_prelude::RpcError> {
                Err(rpc_toolkit_prelude::RpcError {
                    data: Some(method.into()),
                    ..rpc_toolkit_prelude::yajrc::METHOD_NOT_FOUND_ERROR
                })
            }
        },
        Options::Leaf(opt) => {
            let invocation = if opt.is_async {
                quote! {
                    #fn_path(#(#param),*).await?
                }
            } else if opt.blocking.is_some() {
                quote! {
                    rpc_toolkit_prelude::spawn_blocking(move || #fn_path(#(#param),*)).await?
                }
            } else {
                quote! {
                    #fn_path(#(#param),*)?
                }
            };
            quote! {
                #param_struct_def

                pub async fn rpc_handler#fn_generics(
                    ctx: #ctx_ty,
                    method: &str,
                    args: Params#param_ty_generics,
                ) -> Result<rpc_toolkit_prelude::Value, rpc_toolkit_prelude::RpcError> {
                    Ok(rpc_toolkit_prelude::to_value(#invocation)?)
                }
            }
        }
        Options::Parent(ParentOptions {
            common,
            subcommands,
            self_impl,
        }) => {
            let cmd_preprocess = if common.is_async {
                quote! {
                    let ctx = #fn_path(#(#param),*).await?;
                }
            } else if common.blocking.is_some() {
                quote! {
                    let ctx = rpc_toolkit_prelude::spawn_blocking(move || #fn_path(#(#param),*)).await?;
                }
            } else {
                quote! {
                    let ctx = #fn_path(#(#param),*)?;
                }
            };
            let subcmd_impl = subcommands.iter().map(|subcommand| {
                let mut subcommand = subcommand.clone();
                let rpc_handler = PathSegment {
                    ident: Ident::new("rpc_handler", Span::call_site()),
                    arguments: std::mem::replace(
                        &mut subcommand.segments.last_mut().unwrap().arguments,
                        PathArguments::None,
                    ),
                };
                quote_spanned!{ subcommand.span() =>
                    [#subcommand::NAME, rest] => #subcommand::#rpc_handler(ctx, rest, rpc_toolkit_prelude::from_value(args.rest)?).await
                }
            });
            let subcmd_impl = quote! {
                match method.splitn(2, ".").chain(std::iter::repeat("")).take(2).collect::<Vec<_>>().as_slice() {
                    #(
                        #subcmd_impl,
                    )*
                    _ => Err(rpc_toolkit_prelude::RpcError {
                        data: Some(method.into()),
                        ..rpc_toolkit_prelude::yajrc::METHOD_NOT_FOUND_ERROR
                    })
                }
            };
            match self_impl {
                Some(self_impl) if !matches!(common.exec_ctx, ExecutionContext::CliOnly(_)) => {
                    let self_impl_fn = &self_impl.path;
                    let self_impl = if self_impl.is_async {
                        quote_spanned! { self_impl_fn.span() =>
                            #self_impl_fn(ctx).await?
                        }
                    } else if self_impl.blocking {
                        quote_spanned! { self_impl_fn.span() =>
                            rpc_toolkit_prelude::spawn_blocking(move || #self_impl_fn(ctx)).await?
                        }
                    } else {
                        quote_spanned! { self_impl_fn.span() =>
                            #self_impl_fn(ctx)?
                        }
                    };
                    quote! {
                        #param_struct_def

                        pub async fn rpc_handler#fn_generics(
                            ctx: #ctx_ty,
                            method: &str,
                            args: Params#param_ty_generics,
                        ) -> Result<rpc_toolkit_prelude::Value, rpc_toolkit_prelude::RpcError> {
                            #cmd_preprocess

                            if method.is_empty() {
                                Ok(rpc_toolkit_prelude::to_value(&#self_impl)?)
                            } else {
                                #subcmd_impl
                            }
                        }
                    }
                }
                _ => {
                    quote! {
                        #param_struct_def

                        pub async fn rpc_handler#fn_generics(
                            ctx: #ctx_ty,
                            method: &str,
                            args: Params#param_ty_generics,
                        ) -> Result<rpc_toolkit_prelude::Value, rpc_toolkit_prelude::RpcError> {
                            #cmd_preprocess

                            #subcmd_impl
                        }
                    }
                }
            }
        }
    }
}

fn cli_handler(
    fn_name: &Ident,
    fn_generics: &Generics,
    opt: &mut Options,
    params: &[ParamType],
) -> TokenStream {
    let mut ctx_ty = quote! { () };
    for param in params {
        match param {
            ParamType::Context(ctx) => {
                ctx_ty = quote! { #ctx };
            }
            _ => (),
        }
    }
    let mut generics = fn_generics.clone();
    generics.params.push(macro_try!(syn::parse2(
        quote! { ParentParams: rpc_toolkit_prelude::Serialize }
    )));
    if generics.lt_token.is_none() {
        generics.lt_token = Some(Default::default());
    }
    if generics.gt_token.is_none() {
        generics.gt_token = Some(Default::default());
    }
    let (_, fn_type_generics, _) = fn_generics.split_for_impl();
    let fn_turbofish = fn_type_generics.as_turbofish();
    let fn_path: Path = macro_try!(syn::parse2(quote! { super::#fn_name#fn_turbofish }));
    let param = params.iter().map(|param| match param {
        ParamType::Arg(arg) => {
            let name = arg.name.clone().unwrap();
            let field_name = Ident::new(&format!("arg_{}", name), name.span());
            quote! { params.#field_name.clone() }
        }
        ParamType::Context(_) => quote! { ctx },
        _ => unreachable!(),
    });
    let mut param_generics_filter = GenericFilter::new(fn_generics);
    for param in params {
        if let ParamType::Arg(a) = param {
            param_generics_filter.fold_type(a.ty.clone());
        }
    }
    let mut param_generics = param_generics_filter.finish();
    param_generics.params.push(macro_try!(syn::parse2(quote! {
        ParentParams: rpc_toolkit_prelude::Serialize
    })));
    if param_generics.lt_token.is_none() {
        generics.lt_token = Some(Default::default());
    }
    if param_generics.gt_token.is_none() {
        generics.gt_token = Some(Default::default());
    }
    let (_, param_ty_generics, _) = param_generics.split_for_impl();
    let mut arg_def = Vec::new();
    for param in params {
        match param {
            ParamType::Arg(arg) => {
                let name = arg.name.clone().unwrap();
                let rename = LitStr::new(&name.to_string(), name.span());
                let field_name = Ident::new(&format!("arg_{}", name), name.span());
                let ty = arg.ty.clone();
                arg_def.push(quote! {
                    #[serde(rename = #rename)]
                    #field_name: #ty,
                })
            }
            _ => (),
        }
    }
    let arg = params
        .iter()
        .filter_map(|param| {
            if let ParamType::Arg(a) = param {
                Some(a)
            } else {
                None
            }
        })
        .map(|arg| {
            let name = arg.name.clone().unwrap();
            let arg_name = LitStr::new(&name.to_string(), name.span());
            let field_name = Ident::new(&format!("arg_{}", name), name.span());
            if arg.stdin.is_some() {
                if let Some(parse) = &arg.parse {
                    quote! {
                        #field_name: #parse(&mut std::io::stdin(), matches)?,
                    }
                } else {
                    quote! {
                        #field_name: rpc_toolkit_prelude::default_stdin_parser(&mut std::io::stdin(), matches)?,
                    }
                }
            } else if arg.check_is_present {
                quote! {
                    #field_name: matches.is_present(#arg_name),
                }
            } else if arg.count.is_some() {
                quote! {
                    #field_name: matches.occurrences_of(#arg_name),
                }
            } else {
                let parse_val = if let Some(parse) = &arg.parse {
                    quote! {
                        #parse(arg_val, matches)
                    }
                } else {
                    quote! {
                        rpc_toolkit_prelude::default_arg_parser(arg_val, matches)
                    }
                };
                if arg.optional {
                    quote! {
                        #field_name: if let Some(arg_val) = matches.value_of(#arg_name) {
                            Some(#parse_val?)
                        } else {
                            None
                        },
                    }
                } else if arg.multiple.is_some() {
                    quote! {
                        #field_name: matches.values_of(#arg_name).iter().flatten().map(|arg_val| #parse_val).collect::<Result<_, _>>()?,
                    }
                } else {
                    quote! {
                        #field_name: {
                            let arg_val = matches.value_of(#arg_name).unwrap();
                            #parse_val?
                        },
                    }
                }
            }
        });
    let param_struct_def = quote! {
        #[derive(rpc_toolkit_prelude::Serialize)]
        struct Params#param_ty_generics {
            #(
                #arg_def
            )*
            #[serde(flatten)]
            rest: ParentParams,
        }
        let params: Params#param_ty_generics = Params {
            #(
                #arg
            )*
            rest: parent_params,
        };
    };
    let create_rt = quote! {
        let rt_ref = if let Some(rt) = rt.as_mut() {
            &*rt
        } else {
            rt = Some(rpc_toolkit_prelude::Runtime::new().map_err(|e| rpc_toolkit_prelude::RpcError {
                data: Some(format!("{}", e).into()),
                ..rpc_toolkit_prelude::yajrc::INTERNAL_ERROR
            })?);
            rt.as_ref().unwrap()
        };
    };
    let display = if let Some(display) = &opt.common().display {
        quote! { #display }
    } else {
        quote! { rpc_toolkit_prelude::default_display }
    };
    match opt {
        Options::Leaf(opt) if matches!(opt.exec_ctx, ExecutionContext::RpcOnly(_)) => quote! {
            pub fn cli_handler#generics(
                _ctx: #ctx_ty,
                _rt: Option<rpc_toolkit_prelude::Runtime>,
                _matches: &rpc_toolkit_prelude::ArgMatches<'_>,
                method: rpc_toolkit_prelude::Cow<'_, str>,
                _parent_params: ParentParams,
            ) -> Result<(), rpc_toolkit_prelude::RpcError> {
                Err(rpc_toolkit_prelude::RpcError {
                    data: Some(method.into()),
                    ..rpc_toolkit_prelude::yajrc::METHOD_NOT_FOUND_ERROR
                })
            }
        },
        Options::Leaf(opt) if matches!(opt.exec_ctx, ExecutionContext::Standard) => {
            let param = param.map(|_| quote! { unreachable!() });
            let invocation = if opt.is_async {
                quote! {
                    rt_ref.block_on(#fn_path(#(#param),*))?
                }
            } else {
                quote! {
                    #fn_path(#(#param),*)?
                }
            };
            quote! {
                pub fn cli_handler#generics(
                    ctx: #ctx_ty,
                    mut rt: Option<rpc_toolkit_prelude::Runtime>,
                    matches: &rpc_toolkit_prelude::ArgMatches<'_>,
                    method: rpc_toolkit_prelude::Cow<'_, str>,
                    parent_params: ParentParams,
                ) -> Result<(), rpc_toolkit_prelude::RpcError> {
                    #param_struct_def

                    #create_rt

                    #[allow(unreachable_code)]
                    let return_ty = if true {
                        rpc_toolkit_prelude::PhantomData
                    } else {
                        rpc_toolkit_prelude::make_phantom(#invocation)
                    };

                    let res = rt_ref.block_on(rpc_toolkit_prelude::call_remote(ctx, method.as_ref(), params, return_ty))?;
                    Ok(#display(res.result?, matches))
                }
            }
        }
        Options::Leaf(opt) => {
            let invocation = if opt.is_async {
                quote! {
                    rt_ref.block_on(#fn_path(#(#param),*))?
                }
            } else {
                quote! {
                    #fn_path(#(#param),*)?
                }
            };
            let display_res = if let Some(display_fn) = &opt.display {
                quote! {
                    #display_fn(#invocation, matches)
                }
            } else {
                quote! {
                    rpc_toolkit_prelude::default_display(#invocation, matches)
                }
            };
            let rt_action = if opt.is_async {
                create_rt
            } else {
                quote! {
                    drop(rt);
                }
            };
            quote! {
                pub fn cli_handler#generics(
                    ctx: #ctx_ty,
                    mut rt: Option<rpc_toolkit_prelude::Runtime>,
                    matches: &rpc_toolkit_prelude::ArgMatches<'_>,
                    _method: rpc_toolkit_prelude::Cow<'_, str>,
                    _parent_params: ParentParams
                ) -> Result<(), rpc_toolkit_prelude::RpcError> {
                    #rt_action
                    Ok(#display_res)
                }
            }
        }
        Options::Parent(ParentOptions {
            common,
            subcommands,
            self_impl,
        }) => {
            let cmd_preprocess = if common.is_async {
                quote! {
                    #create_rt
                    let ctx = rt_ref.block_on(#fn_path(#(#param),*))?;
                }
            } else {
                quote! {
                    let ctx = #fn_path(#(#param),*)?;
                }
            };
            let subcmd_impl = subcommands.iter().map(|subcommand| {
                let mut subcommand = subcommand.clone();
                let mut cli_handler = PathSegment {
                    ident: Ident::new("cli_handler", Span::call_site()),
                    arguments: std::mem::replace(
                        &mut subcommand.segments.last_mut().unwrap().arguments,
                        PathArguments::None,
                    ),
                };
                cli_handler.arguments = match cli_handler.arguments {
                    PathArguments::None => PathArguments::AngleBracketed(
                        syn::parse2(quote! { ::<Params#param_ty_generics> }).unwrap(),
                    ),
                    PathArguments::AngleBracketed(mut a) => {
                        a.args
                            .push(syn::parse2(quote! { Params#param_ty_generics }).unwrap());
                        PathArguments::AngleBracketed(a)
                    }
                    _ => unreachable!(),
                };
                quote_spanned! { subcommand.span() =>
                    (#subcommand::NAME, Some(sub_m)) => {
                        let method = if method.is_empty() {
                            #subcommand::NAME.into()
                        } else {
                            method + "." + #subcommand::NAME
                        };
                        #subcommand::#cli_handler(ctx, rt, sub_m, method, params)
                    },
                }
            });
            let self_impl = match (self_impl, &common.exec_ctx) {
                (Some(self_impl), ExecutionContext::CliOnly(_)) => {
                    let self_impl_fn = &self_impl.path;
                    let create_rt = if common.is_async {
                        None
                    } else {
                        Some(create_rt)
                    };
                    let self_impl = if self_impl.is_async {
                        quote_spanned! { self_impl_fn.span() =>
                            #create_rt
                            rt_ref.block_on(#self_impl_fn(ctx))?
                        }
                    } else {
                        quote_spanned! { self_impl_fn.span() =>
                            #self_impl_fn(ctx)?
                        }
                    };
                    quote! {
                        Ok(#display(#self_impl, matches)),
                    }
                }
                (Some(self_impl), ExecutionContext::Standard) => {
                    let self_impl_fn = &self_impl.path;
                    let self_impl = if self_impl.is_async {
                        quote! {
                            rt_ref.block_on(#self_impl_fn(ctx))
                        }
                    } else {
                        quote! {
                            #self_impl_fn(ctx)
                        }
                    };
                    let create_rt = if common.is_async {
                        None
                    } else {
                        Some(create_rt)
                    };
                    quote! {
                        {
                            #create_rt

                            #[allow(unreachable_code)]
                            let return_ty = if true {
                                rpc_toolkit_prelude::PhantomData
                            } else {
                                let ctx_new = unreachable!();
                                rpc_toolkit_prelude::match_types(&ctx, &ctx_new);
                                let ctx = ctx_new;
                                rpc_toolkit_prelude::make_phantom(#self_impl?)
                            };

                            let res = rt_ref.block_on(rpc_toolkit_prelude::call_remote(ctx, method.as_ref(), params, return_ty))?;
                            Ok(#display(res.result?, matches))
                        }
                    }
                }
                _ => quote! {
                    Err(rpc_toolkit_prelude::RpcError {
                        data: Some(method.into()),
                        ..rpc_toolkit_prelude::yajrc::METHOD_NOT_FOUND_ERROR
                    }),
                },
            };
            quote! {
                pub fn cli_handler#generics(
                    ctx: #ctx_ty,
                    mut rt: Option<rpc_toolkit_prelude::Runtime>,
                    matches: &rpc_toolkit_prelude::ArgMatches<'_>,
                    method: rpc_toolkit_prelude::Cow<'_, str>,
                    parent_params: ParentParams,
                ) -> Result<(), rpc_toolkit_prelude::RpcError> {
                    #param_struct_def

                    #cmd_preprocess

                    match matches.subcommand() {
                        #(
                            #subcmd_impl
                        )*
                        _ => #self_impl
                    }
                }
            }
        }
    }
}

pub fn build(args: AttributeArgs, mut item: ItemFn) -> TokenStream {
    let mut params = macro_try!(parse_param_attrs(&mut item));
    let mut opt = macro_try!(parse_command_attr(args));
    if let Some(a) = &opt.common().blocking {
        if item.sig.asyncness.is_some() {
            return Error::new(a.span(), "cannot use `blocking` on an async fn").to_compile_error();
        }
    }
    opt.common().is_async = item.sig.asyncness.is_some();
    let fn_vis = &item.vis;
    let fn_name = &item.sig.ident;
    let fn_generics = &item.sig.generics;
    let command_name = opt
        .common()
        .rename
        .clone()
        .unwrap_or_else(|| fn_name.clone());
    let command_name_str = LitStr::new(&command_name.to_string(), command_name.span());
    let is_async = LitBool::new(
        opt.common().is_async,
        item.sig
            .asyncness
            .map(|a| a.span())
            .unwrap_or_else(Span::call_site),
    );
    let build_app = build_app(command_name_str.clone(), &mut opt, &mut params);
    let rpc_handler = rpc_handler(fn_name, fn_generics, &opt, &params);
    let cli_handler = cli_handler(fn_name, fn_generics, &mut opt, &params);

    let res = quote! {
        #item
        #fn_vis mod #fn_name {
            use super::*;
            use rpc_toolkit::command_helpers::prelude as rpc_toolkit_prelude;

            pub const NAME: &'static str = #command_name_str;
            pub const ASYNC: bool = #is_async;

            #build_app

            #rpc_handler

            #cli_handler
        }
    };
    // panic!("{}", res);
    res
}
