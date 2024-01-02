use std::collections::HashSet;

use itertools::MultiUnzip;
use proc_macro2::*;
use quote::*;
use syn::fold::Fold;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::{Add, Comma, Where};

use super::parse::*;
use super::*;

// fn ctx_trait(ctx_ty: Option<Type>, opt: &mut Options) -> TokenStream {
//     let mut bounds: Punctuated<TypeParamBound, Add> = Punctuated::new();
//     bounds.push(macro_try!(parse2(quote! { ::rpc_toolkit::Context })));
//     let mut rpc_bounds = bounds.clone();
//     let mut cli_bounds = bounds;

//     let (use_cli, use_rpc) = match &opt.common().exec_ctx {
//         ExecutionContext::CliOnly(_) => (Some(None), false),
//         ExecutionContext::RpcOnly(_) | ExecutionContext::Standard => (None, true),
//         ExecutionContext::Local(_) => (Some(None), true),
//         ExecutionContext::CustomCli { context, .. } => (Some(Some(context.clone())), true),
//     };

//     if let Options::Parent(ParentOptions {
//         subcommands,
//         self_impl,
//         ..
//     }) = opt
//     {
//         if let Some(ctx_ty) = ctx_ty {
//             cli_bounds.push(macro_try!(parse2(quote! { Into<#ctx_ty> })));
//             cli_bounds.push(macro_try!(parse2(quote! { Clone })));
//             rpc_bounds.push(macro_try!(parse2(quote! { Into<#ctx_ty> })));
//             rpc_bounds.push(macro_try!(parse2(quote! { Clone })));
//         }
//         if let Some(SelfImplInfo { context, .. }) = self_impl {
//             if let Some(cli_ty) = use_cli.as_ref() {
//                 if let Some(cli_ty) = cli_ty {
//                     cli_bounds.push(macro_try!(parse2(quote! { Into<#cli_ty> })));
//                 } else {
//                     cli_bounds.push(macro_try!(parse2(quote! { Into<#context> })));
//                 }
//             }
//             if use_rpc {
//                 rpc_bounds.push(macro_try!(parse2(quote! { Into<#context> })));
//             }
//         }
//         for subcmd in subcommands {
//             let mut path = subcmd.clone();
//             std::mem::take(&mut path.segments.last_mut().unwrap().arguments);
//             cli_bounds.push(macro_try!(parse2(quote! { #path::CommandContextCli })));
//             rpc_bounds.push(macro_try!(parse2(quote! { #path::CommandContextRpc })));
//         }
//     } else {
//         if let Some(cli_ty) = use_cli.as_ref() {
//             if let Some(cli_ty) = cli_ty {
//                 cli_bounds.push(macro_try!(parse2(quote! { Into<#cli_ty> })));
//             } else if let Some(ctx_ty) = &ctx_ty {
//                 cli_bounds.push(macro_try!(parse2(quote! { Into<#ctx_ty> })));
//             }
//         }
//         if use_rpc {
//             if let Some(ctx_ty) = &ctx_ty {
//                 rpc_bounds.push(macro_try!(parse2(quote! { Into<#ctx_ty> })));
//             }
//         }
//     }

//     let res = quote! {
//         pub trait CommandContextCli: #cli_bounds {}
//         impl<T> CommandContextCli for T where T: #cli_bounds {}

//         pub trait CommandContextRpc: #rpc_bounds {}
//         impl<T> CommandContextRpc for T where T: #rpc_bounds {}
//     };
//     res
// }

// fn metadata(full_options: &Options) -> TokenStream {
//     let options = match full_options {
//         Options::Leaf(a) => a,
//         Options::Parent(ParentOptions { common, .. }) => common,
//     };
//     let fallthrough = |ty: &str| {
//         let getter_name = Ident::new(&format!("get_{}", ty), Span::call_site());
//         match &*full_options {
//             Options::Parent(ParentOptions { subcommands, .. }) => {
//                 let subcmd_handler = subcommands.iter().map(|subcmd| {
//                     let mut subcmd = subcmd.clone();
//                     subcmd.segments.last_mut().unwrap().arguments = PathArguments::None;
//                     quote_spanned!{ subcmd.span() =>
//                         [#subcmd::NAME, rest] => if let Some(val) = #subcmd::Metadata.#getter_name(rest, key) {
//                             return Some(val);
//                         },
//                     }
//                 });
//                 quote! {
//                     if !command.is_empty() {
//                         match command.splitn(2, ".").chain(std::iter::repeat("")).take(2).collect::<Vec<_>>().as_slice() {
//                             #(
//                                 #subcmd_handler
//                             )*
//                             _ => ()
//                         }
//                     }
//                 }
//             }
//             _ => quote! {},
//         }
//     };
//     fn impl_getter<I: Iterator<Item = TokenStream>>(
//         ty: &str,
//         metadata: I,
//         fallthrough: TokenStream,
//     ) -> TokenStream {
//         let getter_name = Ident::new(&format!("get_{}", ty), Span::call_site());
//         let ty: Type = syn::parse_str(ty).unwrap();
//         quote! {
//             fn #getter_name(&self, command: &str, key: &str) -> Option<#ty> {
//                 #fallthrough
//                 match key {
//                     #(#metadata)*
//                     _ => None,
//                 }
//             }
//         }
//     }
//     let bool_metadata = options
//         .metadata
//         .iter()
//         .filter(|(_, lit)| matches!(lit, Lit::Bool(_)))
//         .map(|(name, value)| {
//             let name = LitStr::new(&name.to_string(), name.span());
//             quote! {
//                 #name => Some(#value),
//             }
//         });
//     let number_metadata = |ty: &str| {
//         let ty: Type = syn::parse_str(ty).unwrap();
//         options
//             .metadata
//             .iter()
//             .filter(|(_, lit)| matches!(lit, Lit::Int(_) | Lit::Float(_) | Lit::Byte(_)))
//             .map(move |(name, value)| {
//                 let name = LitStr::new(&name.to_string(), name.span());
//                 quote! {
//                     #name => Some(#value as #ty),
//                 }
//             })
//     };
//     let char_metadata = options
//         .metadata
//         .iter()
//         .filter(|(_, lit)| matches!(lit, Lit::Char(_)))
//         .map(|(name, value)| {
//             let name = LitStr::new(&name.to_string(), name.span());
//             quote! {
//                 #name => Some(#value),
//             }
//         });
//     let str_metadata = options
//         .metadata
//         .iter()
//         .filter(|(_, lit)| matches!(lit, Lit::Str(_)))
//         .map(|(name, value)| {
//             let name = LitStr::new(&name.to_string(), name.span());
//             quote! {
//                 #name => Some(#value),
//             }
//         });
//     let bstr_metadata = options
//         .metadata
//         .iter()
//         .filter(|(_, lit)| matches!(lit, Lit::ByteStr(_)))
//         .map(|(name, value)| {
//             let name = LitStr::new(&name.to_string(), name.span());
//             quote! {
//                 #name => Some(#value),
//             }
//         });

//     let bool_getter = impl_getter("bool", bool_metadata, fallthrough("bool"));
//     let u8_getter = impl_getter("u8", number_metadata("u8"), fallthrough("u8"));
//     let u16_getter = impl_getter("u16", number_metadata("u16"), fallthrough("u16"));
//     let u32_getter = impl_getter("u32", number_metadata("u32"), fallthrough("u32"));
//     let u64_getter = impl_getter("u64", number_metadata("u64"), fallthrough("u64"));
//     let usize_getter = impl_getter("usize", number_metadata("usize"), fallthrough("usize"));
//     let i8_getter = impl_getter("i8", number_metadata("i8"), fallthrough("i8"));
//     let i16_getter = impl_getter("i16", number_metadata("i16"), fallthrough("i16"));
//     let i32_getter = impl_getter("i32", number_metadata("i32"), fallthrough("i32"));
//     let i64_getter = impl_getter("i64", number_metadata("i64"), fallthrough("i64"));
//     let isize_getter = impl_getter("isize", number_metadata("isize"), fallthrough("isize"));
//     let f32_getter = impl_getter("f32", number_metadata("f32"), fallthrough("f32"));
//     let f64_getter = impl_getter("f64", number_metadata("f64"), fallthrough("f64"));
//     let char_getter = impl_getter("char", char_metadata, fallthrough("char"));
//     let str_fallthrough = fallthrough("str");
//     let str_getter = quote! {
//         fn get_str(&self, command: &str, key: &str) -> Option<&'static str> {
//             #str_fallthrough
//             match key {
//                 #(#str_metadata)*
//                 _ => None,
//             }
//         }
//     };
//     let bstr_fallthrough = fallthrough("bstr");
//     let bstr_getter = quote! {
//         fn get_bstr(&self, command: &str, key: &str) -> Option<&'static [u8]> {
//             #bstr_fallthrough
//             match key {
//                 #(#bstr_metadata)*
//                 _ => None,
//             }
//         }
//     };

//     let res = quote! {
//         #[derive(Clone, Copy, Default)]
//         pub struct Metadata;

//         #[allow(overflowing_literals)]
//         impl ::rpc_toolkit::Metadata for Metadata {
//             #bool_getter
//             #u8_getter
//             #u16_getter
//             #u32_getter
//             #u64_getter
//             #usize_getter
//             #i8_getter
//             #i16_getter
//             #i32_getter
//             #i64_getter
//             #isize_getter
//             #f32_getter
//             #f64_getter
//             #char_getter
//             #str_getter
//             #bstr_getter
//         }
//     };
//     // panic!("{}", res);
//     res
// }

// fn build_app(name: LitStr, opt: &mut Options, params: &mut [ParamType]) -> TokenStream {
//     let about = opt.common().about.clone().into_iter();
//     let (subcommand, subcommand_required) = if let Options::Parent(opt) = opt {
//         (
//             opt.subcommands
//                 .iter()
//                 .map(|subcmd| {
//                     let mut path = subcmd.clone();
//                     path.segments.last_mut().unwrap().arguments = PathArguments::None;
//                     path
//                 })
//                 .collect(),
//             opt.self_impl.is_none(),
//         )
//     } else {
//         (Vec::new(), false)
//     };
//     let arg = params
//         .iter_mut()
//         .filter_map(|param| {
//             if let ParamType::Arg(arg) = param {
//                 if arg.stdin.is_some() {
//                     return None;
//                 }
//                 let name = arg.name.clone().unwrap();
//                 let name_str = arg.rename.clone().unwrap_or_else(|| LitStr::new(&name.to_string(), name.span()));
//                 let help = arg.help.clone().into_iter();
//                 let short = arg.short.clone().into_iter();
//                 let long = arg.long.clone().into_iter();
//                 let mut modifications = TokenStream::default();
//                 let ty_span = arg.ty.span();
//                 if let Type::Path(p) = &mut arg.ty {
//                     if p.path.is_ident("bool")
//                         && arg.parse.is_none()
//                         && (arg.short.is_some() || arg.long.is_some())
//                     {
//                         arg.check_is_present = true;
//                         modifications.extend(quote_spanned! { ty_span =>
//                             arg = arg.takes_value(false);
//                         });
//                     } else if arg.count.is_some() {
//                         modifications.extend(quote_spanned! { ty_span =>
//                             arg = arg.takes_value(false);
//                             arg = arg.multiple(true);
//                         });
//                     } else {
//                         modifications.extend(quote_spanned! { ty_span =>
//                             arg = arg.takes_value(true);
//                         });
//                         if let Some(_) = &arg.default {
//                             modifications.extend(quote_spanned! { ty_span =>
//                                 arg = arg.required(false);
//                             });
//                         } else if p.path.segments.last().unwrap().ident == "Option" {
//                             arg.optional = true;
//                             modifications.extend(quote_spanned! { ty_span =>
//                                 arg = arg.required(false);
//                             });
//                         } else if arg.multiple.is_some() {
//                             modifications.extend(quote_spanned! { ty_span =>
//                                 arg = arg.multiple(true);
//                             });
//                         } else {
//                             modifications.extend(quote_spanned! { ty_span =>
//                                 arg = arg.required(true);
//                             });
//                         }
//                     }
//                 };
//                 Some(quote! {
//                     {
//                         let mut arg = ::rpc_toolkit::command_helpers::prelude::Arg::with_name(#name_str);
//                         #(
//                             arg = arg.help(#help);
//                         )*
//                         #(
//                             arg = arg.short(#short);
//                         )*
//                         #(
//                             arg = arg.long(#long);
//                         )*
//                         #modifications

//                         arg
//                     }
//                 })
//             } else {
//                 None
//             }
//         })
//         .collect::<Vec<_>>();
//     let required = LitBool::new(subcommand_required, Span::call_site());
//     let alias = &opt.common().aliases;
//     quote! {
//         pub fn build_app() -> ::rpc_toolkit::command_helpers::prelude::App<'static> {
//             let mut app = ::rpc_toolkit::command_helpers::prelude::App::new(#name);
//             #(
//                 app = app.about(#about);
//             )*
//             #(
//                 app = app.alias(#alias);
//             )*
//             #(
//                 app = app.arg(#arg);
//             )*
//             #(
//                 app = app.subcommand(#subcommand::build_app());
//             )*
//             if #required {
//                 app = app.setting(::rpc_toolkit::command_helpers::prelude::AppSettings::SubcommandRequired);
//             }
//             app
//         }
//     }
// }

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
                }) if self.types.contains(&t.path.segments.first().unwrap().ident) => {
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
                    (GenericParam::Type(t), Type::Path(i))
                        if &i.path.segments.first().unwrap().ident == &t.ident =>
                    {
                        Some(t.ident.clone())
                    }
                    _ => None,
                }),
        );
        i
    }
}

// fn rpc_handler(
//     fn_name: &Ident,
//     fn_generics: &Generics,
//     opt: &Options,
//     params: &[ParamType],
// ) -> TokenStream {
//     let mut parent_data_ty = quote! { () };
//     let mut generics = fn_generics.clone();
//     generics.params.push(macro_try!(syn::parse2(
//         quote! { GenericContext: CommandContextRpc }
//     )));
//     if generics.lt_token.is_none() {
//         generics.lt_token = Some(Default::default());
//     }
//     if generics.gt_token.is_none() {
//         generics.gt_token = Some(Default::default());
//     }
//     let mut param_def = Vec::new();
//     for param in params {
//         match param {
//             ParamType::Arg(arg) => {
//                 let name = arg.name.clone().unwrap();
//                 let rename = arg
//                     .rename
//                     .clone()
//                     .unwrap_or_else(|| LitStr::new(&name.to_string(), name.span()));
//                 let field_name = Ident::new(&format!("arg_{}", name), name.span());
//                 let ty = arg.ty.clone();
//                 let def = quote! {
//                     #[serde(rename = #rename)]
//                     #field_name: #ty,
//                 };
//                 let def = match &arg.default {
//                     Some(Some(default)) => {
//                         quote! {
//                             #[serde(default = #default)]
//                             #def
//                         }
//                     }
//                     Some(None) => {
//                         quote! {
//                             #[serde(default)]
//                             #def
//                         }
//                     }
//                     None => def,
//                 };
//                 param_def.push(def);
//             }
//             ParamType::ParentData(ty) => parent_data_ty = quote! { #ty },
//             _ => (),
//         }
//     }
//     let (_, fn_type_generics, _) = fn_generics.split_for_impl();
//     let fn_turbofish = fn_type_generics.as_turbofish();
//     let fn_path: Path = macro_try!(syn::parse2(quote! { super::#fn_name#fn_turbofish }));
//     let mut param_generics_filter = GenericFilter::new(fn_generics);
//     for param in params {
//         if let ParamType::Arg(a) = param {
//             param_generics_filter.fold_type(a.ty.clone());
//         }
//     }
//     let param_generics = param_generics_filter.finish();
//     let (_, param_ty_generics, _) = param_generics.split_for_impl();
//     let param_struct_def = quote! {
//         #[allow(dead_code)]
//         #[derive(::rpc_toolkit::command_helpers::prelude::Deserialize)]
//         pub struct Params#param_ty_generics {
//             #(
//                 #param_def
//             )*
//             #[serde(flatten)]
//             #[serde(default)]
//             rest: ::rpc_toolkit::command_helpers::prelude::Value,
//         }
//     };
//     let param = params.iter().map(|param| match param {
//         ParamType::Arg(arg) => {
//             let name = arg.name.clone().unwrap();
//             let field_name = Ident::new(&format!("arg_{}", name), name.span());
//             quote! { args.#field_name }
//         }
//         ParamType::Context(ty) => {
//             if matches!(opt, Options::Parent { .. }) {
//                 quote! { <GenericContext as Into<#ty>>::into(ctx.clone()) }
//             } else {
//                 quote! { <GenericContext as Into<#ty>>::into(ctx) }
//             }
//         }
//         ParamType::ParentData(_) => {
//             quote! { parent_data }
//         }
//         ParamType::Request => quote! { request },
//         ParamType::Response => quote! { response },
//         ParamType::None => unreachable!(),
//     });
//     match opt {
//         Options::Leaf(opt) if matches!(opt.exec_ctx, ExecutionContext::CliOnly(_)) => quote! {
//             #param_struct_def

//             pub async fn rpc_handler#generics(
//                 _ctx: GenericContext,
//                 _parent_data: #parent_data_ty,
//                 _request: &::rpc_toolkit::command_helpers::prelude::RequestParts,
//                 _response: &mut ::rpc_toolkit::command_helpers::prelude::ResponseParts,
//                 method: &str,
//                 _args: Params#param_ty_generics,
//             ) -> Result<::rpc_toolkit::command_helpers::prelude::Value, ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                 Err(::rpc_toolkit::command_helpers::prelude::RpcError {
//                     data: Some(method.into()),
//                     ..::rpc_toolkit::command_helpers::prelude::yajrc::METHOD_NOT_FOUND_ERROR
//                 })
//             }
//         },
//         Options::Leaf(opt) => {
//             let invocation = if opt.is_async {
//                 quote! {
//                     #fn_path(#(#param),*).await?
//                 }
//             } else if opt.blocking.is_some() {
//                 quote! {
//                     ::rpc_toolkit::command_helpers::prelude::spawn_blocking(move || #fn_path(#(#param),*)).await?
//                 }
//             } else {
//                 quote! {
//                     #fn_path(#(#param),*)?
//                 }
//             };
//             quote! {
//                 #param_struct_def

//                 pub async fn rpc_handler#generics(
//                     ctx: GenericContext,
//                     parent_data: #parent_data_ty,
//                     request: &::rpc_toolkit::command_helpers::prelude::RequestParts,
//                     response: &mut ::rpc_toolkit::command_helpers::prelude::ResponseParts,
//                     method: &str,
//                     args: Params#param_ty_generics,
//                 ) -> Result<::rpc_toolkit::command_helpers::prelude::Value, ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                     if method.is_empty() {
//                         Ok(::rpc_toolkit::command_helpers::prelude::to_value(#invocation)?)
//                     } else {
//                         Err(::rpc_toolkit::command_helpers::prelude::RpcError {
//                             data: Some(method.into()),
//                             ..::rpc_toolkit::command_helpers::prelude::yajrc::METHOD_NOT_FOUND_ERROR
//                         })
//                     }
//                 }
//             }
//         }
//         Options::Parent(ParentOptions {
//             common,
//             subcommands,
//             self_impl,
//         }) => {
//             let cmd_preprocess = if common.is_async {
//                 quote! {
//                     let parent_data = #fn_path(#(#param),*).await?;
//                 }
//             } else if common.blocking.is_some() {
//                 quote! {
//                     let parent_data = ::rpc_toolkit::command_helpers::prelude::spawn_blocking(move || #fn_path(#(#param),*)).await?;
//                 }
//             } else {
//                 quote! {
//                     let parent_data = #fn_path(#(#param),*)?;
//                 }
//             };
//             let subcmd_impl = subcommands.iter().map(|subcommand| {
//                 let mut subcommand = subcommand.clone();
//                 let mut rpc_handler = PathSegment {
//                     ident: Ident::new("rpc_handler", Span::call_site()),
//                     arguments: std::mem::replace(
//                         &mut subcommand.segments.last_mut().unwrap().arguments,
//                         PathArguments::None,
//                     ),
//                 };
//                 rpc_handler.arguments = match rpc_handler.arguments {
//                     PathArguments::None => PathArguments::AngleBracketed(
//                         syn::parse2(quote! { ::<GenericContext> })
//                             .unwrap(),
//                     ),
//                     PathArguments::AngleBracketed(mut a) => {
//                         a.args.push(syn::parse2(quote! { GenericContext }).unwrap());
//                         PathArguments::AngleBracketed(a)
//                     }
//                     _ => unreachable!(),
//                 };
//                 quote_spanned!{ subcommand.span() =>
//                     [#subcommand::NAME, rest] => #subcommand::#rpc_handler(ctx, parent_data, request, response, rest, ::rpc_toolkit::command_helpers::prelude::from_value(args.rest)?).await
//                 }
//             });
//             let subcmd_impl = quote! {
//                 match method.splitn(2, ".").chain(std::iter::repeat("")).take(2).collect::<Vec<_>>().as_slice() {
//                     #(
//                         #subcmd_impl,
//                     )*
//                     _ => Err(::rpc_toolkit::command_helpers::prelude::RpcError {
//                         data: Some(method.into()),
//                         ..::rpc_toolkit::command_helpers::prelude::yajrc::METHOD_NOT_FOUND_ERROR
//                     })
//                 }
//             };
//             match self_impl {
//                 Some(self_impl) if !matches!(common.exec_ctx, ExecutionContext::CliOnly(_)) => {
//                     let self_impl_fn = &self_impl.path;
//                     let self_impl = if self_impl.is_async {
//                         quote_spanned! { self_impl_fn.span() =>
//                             #self_impl_fn(Into::into(ctx), parent_data).await?
//                         }
//                     } else if self_impl.blocking {
//                         quote_spanned! { self_impl_fn.span() =>
//                             {
//                                 let ctx = Into::into(ctx);
//                                 ::rpc_toolkit::command_helpers::prelude::spawn_blocking(move || #self_impl_fn(ctx, parent_data)).await?
//                             }
//                         }
//                     } else {
//                         quote_spanned! { self_impl_fn.span() =>
//                             #self_impl_fn(Into::into(ctx), parent_data)?
//                         }
//                     };
//                     quote! {
//                         #param_struct_def

//                         pub async fn rpc_handler#generics(
//                             ctx: GenericContext,
//                             parent_data: #parent_data_ty,
//                             request: &::rpc_toolkit::command_helpers::prelude::RequestParts,
//                             response: &mut ::rpc_toolkit::command_helpers::prelude::ResponseParts,
//                             method: &str,
//                             args: Params#param_ty_generics,
//                         ) -> Result<::rpc_toolkit::command_helpers::prelude::Value, ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                             #cmd_preprocess

//                             if method.is_empty() {
//                                 Ok(::rpc_toolkit::command_helpers::prelude::to_value(&#self_impl)?)
//                             } else {
//                                 #subcmd_impl
//                             }
//                         }
//                     }
//                 }
//                 _ => {
//                     quote! {
//                         #param_struct_def

//                         pub async fn rpc_handler#generics(
//                             ctx: GenericContext,
//                             parent_data: #parent_data_ty,
//                             request: &::rpc_toolkit::command_helpers::prelude::RequestParts,
//                             response: &mut ::rpc_toolkit::command_helpers::prelude::ResponseParts,
//                             method: &str,
//                             args: Params#param_ty_generics,
//                         ) -> Result<::rpc_toolkit::command_helpers::prelude::Value, ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                             #cmd_preprocess

//                             #subcmd_impl
//                         }
//                     }
//                 }
//             }
//         }
//     }
// }

// fn cli_handler(
//     fn_name: &Ident,
//     fn_generics: &Generics,
//     opt: &mut Options,
//     params: &[ParamType],
// ) -> TokenStream {
//     let mut parent_data_ty = quote! { () };
//     let mut generics = fn_generics.clone();
//     generics.params.push(macro_try!(syn::parse2(
//         quote! { ParentParams: ::rpc_toolkit::command_helpers::prelude::Serialize }
//     )));
//     generics.params.push(macro_try!(syn::parse2(
//         quote! { GenericContext: CommandContextCli }
//     )));
//     if generics.lt_token.is_none() {
//         generics.lt_token = Some(Default::default());
//     }
//     if generics.gt_token.is_none() {
//         generics.gt_token = Some(Default::default());
//     }
//     let (_, fn_type_generics, _) = fn_generics.split_for_impl();
//     let fn_turbofish = fn_type_generics.as_turbofish();
//     let fn_path: Path = macro_try!(syn::parse2(quote! { super::#fn_name#fn_turbofish }));
//     let is_parent = matches!(opt, Options::Parent { .. });
//     let param: Vec<_> = params
//         .iter()
//         .map(|param| match param {
//             ParamType::Arg(arg) => {
//                 let name = arg.name.clone().unwrap();
//                 let field_name = Ident::new(&format!("arg_{}", name), name.span());
//                 quote! { params.#field_name.clone() }
//             }
//             ParamType::Context(ty) => {
//                 if is_parent {
//                     quote! { <GenericContext as Into<#ty>>::into(ctx.clone()) }
//                 } else {
//                     quote! { <GenericContext as Into<#ty>>::into(ctx) }
//                 }
//             }
//             ParamType::ParentData(ty) => {
//                 parent_data_ty = quote! { #ty };
//                 quote! { parent_data }
//             }
//             ParamType::Request => quote! { request },
//             ParamType::Response => quote! { response },
//             ParamType::None => unreachable!(),
//         })
//         .collect();
//     let mut param_generics_filter = GenericFilter::new(fn_generics);
//     for param in params {
//         if let ParamType::Arg(a) = param {
//             param_generics_filter.fold_type(a.ty.clone());
//         }
//     }
//     let mut param_generics = param_generics_filter.finish();
//     param_generics.params.push(macro_try!(syn::parse2(quote! {
//         ParentParams: ::rpc_toolkit::command_helpers::prelude::Serialize
//     })));
//     if param_generics.lt_token.is_none() {
//         param_generics.lt_token = Some(Default::default());
//     }
//     if param_generics.gt_token.is_none() {
//         param_generics.gt_token = Some(Default::default());
//     }
//     let (_, param_ty_generics, _) = param_generics.split_for_impl();
//     let mut arg_def = Vec::new();
//     for param in params {
//         match param {
//             ParamType::Arg(arg) => {
//                 let name = arg.name.clone().unwrap();
//                 let rename = arg
//                     .rename
//                     .clone()
//                     .unwrap_or_else(|| LitStr::new(&name.to_string(), name.span()));
//                 let field_name = Ident::new(&format!("arg_{}", name), name.span());
//                 let ty = arg.ty.clone();
//                 arg_def.push(quote! {
//                     #[serde(rename = #rename)]
//                     #field_name: #ty,
//                 })
//             }
//             _ => (),
//         }
//     }
//     let arg = params
//         .iter()
//         .filter_map(|param| {
//             if let ParamType::Arg(a) = param {
//                 Some(a)
//             } else {
//                 None
//             }
//         })
//         .map(|arg| {
//             let name = arg.name.clone().unwrap();
//             let arg_name = arg.rename.clone().unwrap_or_else(|| LitStr::new(&name.to_string(), name.span()));
//             let field_name = Ident::new(&format!("arg_{}", name), name.span());
//             if arg.stdin.is_some() {
//                 if let Some(parse) = &arg.parse {
//                     quote! {
//                         #field_name: #parse(&mut std::io::stdin(), matches)?,
//                     }
//                 } else {
//                     quote! {
//                         #field_name: ::rpc_toolkit::command_helpers::prelude::default_stdin_parser(&mut std::io::stdin(), matches)?,
//                     }
//                 }
//             } else if arg.check_is_present {
//                 quote! {
//                     #field_name: matches.is_present(#arg_name),
//                 }
//             } else if arg.count.is_some() {
//                 quote! {
//                     #field_name: matches.occurrences_of(#arg_name),
//                 }
//             } else {
//                 let parse_val = if let Some(parse) = &arg.parse {
//                     quote! {
//                         #parse(arg_val, matches)
//                     }
//                 } else {
//                     quote! {
//                         ::rpc_toolkit::command_helpers::prelude::default_arg_parser(arg_val, matches)
//                     }
//                 };
//                 if arg.optional {
//                     quote! {
//                         #field_name: if let Some(arg_val) = matches.value_of(#arg_name) {
//                             Some(#parse_val?)
//                         } else {
//                             None
//                         },
//                     }
//                 } else if let Some(default) = &arg.default {
//                     if let Some(default) = default {
//                         let path: Path = match syn::parse_str(&default.value()) {
//                             Ok(a) => a,
//                             Err(e) => return e.into_compile_error(),
//                         };
//                         quote! {
//                             #field_name: if let Some(arg_val) = matches.value_of(#arg_name) {
//                                 #parse_val?
//                             } else {
//                                 #path()
//                             },
//                         }
//                     } else {
//                         quote! {
//                             #field_name: if let Some(arg_val) = matches.value_of(#arg_name) {
//                                 #parse_val?
//                             } else {
//                                 Default::default()
//                             },
//                         }
//                     }
//                 } else if arg.multiple.is_some() {
//                     quote! {
//                         #field_name: matches.values_of(#arg_name).iter().flatten().map(|arg_val| #parse_val).collect::<Result<_, _>>()?,
//                     }
//                 } else {
//                     quote! {
//                         #field_name: {
//                             let arg_val = matches.value_of(#arg_name).unwrap();
//                             #parse_val?
//                         },
//                     }
//                 }
//             }
//         });
//     let param_struct_def = quote! {
//         #[derive(::rpc_toolkit::command_helpers::prelude::Serialize)]
//         struct Params#param_ty_generics {
//             #(
//                 #arg_def
//             )*
//             #[serde(flatten)]
//             rest: ParentParams,
//         }
//         let params: Params#param_ty_generics = Params {
//             #(
//                 #arg
//             )*
//             rest: parent_params,
//         };
//     };
//     let create_rt = quote! {
//         let rt_ref = if let Some(rt) = rt.as_mut() {
//             &*rt
//         } else {
//             rt = Some(::rpc_toolkit::command_helpers::prelude::Runtime::new().map_err(|e| ::rpc_toolkit::command_helpers::prelude::RpcError {
//                 data: Some(format!("{}", e).into()),
//                 ..::rpc_toolkit::command_helpers::prelude::yajrc::INTERNAL_ERROR
//             })?);
//             rt.as_ref().unwrap()
//         };
//     };
//     let display = if let Some(display) = &opt.common().display {
//         quote! { #display }
//     } else {
//         quote! { ::rpc_toolkit::command_helpers::prelude::default_display }
//     };
//     match opt {
//         Options::Leaf(opt) if matches!(opt.exec_ctx, ExecutionContext::RpcOnly(_)) => quote! {
//             pub fn cli_handler#generics(
//                 _ctx: GenericContext,
//                 _parent_data: #parent_data_ty,
//                 _rt: Option<::rpc_toolkit::command_helpers::prelude::Runtime>,
//                 _matches: &::rpc_toolkit::command_helpers::prelude::ArgMatches,
//                 method: ::rpc_toolkit::command_helpers::prelude::Cow<'_, str>,
//                 _parent_params: ParentParams,
//             ) -> Result<(), ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                 Err(::rpc_toolkit::command_helpers::prelude::RpcError {
//                     data: Some(method.into()),
//                     ..::rpc_toolkit::command_helpers::prelude::yajrc::METHOD_NOT_FOUND_ERROR
//                 })
//             }
//         },
//         Options::Leaf(opt) if matches!(opt.exec_ctx, ExecutionContext::Standard) => {
//             let param = param.into_iter().map(|_| quote! { unreachable!() });
//             let invocation = if opt.is_async {
//                 quote! {
//                     rt_ref.block_on(#fn_path(#(#param),*))?
//                 }
//             } else {
//                 quote! {
//                     #fn_path(#(#param),*)?
//                 }
//             };
//             quote! {
//                 pub fn cli_handler#generics(
//                     ctx: GenericContext,
//                     parent_data: #parent_data_ty,
//                     mut rt: Option<::rpc_toolkit::command_helpers::prelude::Runtime>,
//                     matches: &::rpc_toolkit::command_helpers::prelude::ArgMatches,
//                     method: ::rpc_toolkit::command_helpers::prelude::Cow<'_, str>,
//                     parent_params: ParentParams,
//                 ) -> Result<(), ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                     #param_struct_def

//                     #create_rt

//                     #[allow(unreachable_code)]
//                     let return_ty = if true {
//                         ::rpc_toolkit::command_helpers::prelude::PhantomData
//                     } else {
//                         let ctx_new = unreachable!();
//                         ::rpc_toolkit::command_helpers::prelude::match_types(&ctx, &ctx_new);
//                         let ctx = ctx_new;
//                         ::rpc_toolkit::command_helpers::prelude::make_phantom(#invocation)
//                     };

//                     let res = rt_ref.block_on(::rpc_toolkit::command_helpers::prelude::call_remote(ctx, method.as_ref(), params, return_ty))?;
//                     Ok(#display(res.result?, matches))
//                 }
//             }
//         }
//         Options::Leaf(opt) => {
//             if let ExecutionContext::CustomCli {
//                 ref cli, is_async, ..
//             } = opt.exec_ctx
//             {
//                 let fn_path = cli;
//                 let cli_param = params.iter().filter_map(|param| match param {
//                     ParamType::Arg(arg) => {
//                         let name = arg.name.clone().unwrap();
//                         let field_name = Ident::new(&format!("arg_{}", name), name.span());
//                         Some(quote! { params.#field_name.clone() })
//                     }
//                     ParamType::Context(_) => Some(quote! { Into::into(ctx) }),
//                     ParamType::ParentData(_) => Some(quote! { parent_data }),
//                     ParamType::Request => None,
//                     ParamType::Response => None,
//                     ParamType::None => unreachable!(),
//                 });
//                 let invocation = if is_async {
//                     quote! {
//                         rt_ref.block_on(#fn_path(#(#cli_param),*))?
//                     }
//                 } else {
//                     quote! {
//                         #fn_path(#(#cli_param),*)?
//                     }
//                 };
//                 let display_res = if let Some(display_fn) = &opt.display {
//                     quote! {
//                         #display_fn(#invocation, matches)
//                     }
//                 } else {
//                     quote! {
//                         ::rpc_toolkit::command_helpers::prelude::default_display(#invocation, matches)
//                     }
//                 };
//                 let rt_action = if is_async {
//                     create_rt
//                 } else {
//                     quote! {
//                         drop(rt);
//                     }
//                 };
//                 quote! {
//                     pub fn cli_handler#generics(
//                         ctx: GenericContext,
//                         parent_data: #parent_data_ty,
//                         mut rt: Option<::rpc_toolkit::command_helpers::prelude::Runtime>,
//                         matches: &::rpc_toolkit::command_helpers::prelude::ArgMatches,
//                         _method: ::rpc_toolkit::command_helpers::prelude::Cow<'_, str>,
//                         parent_params: ParentParams
//                     ) -> Result<(), ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                         #param_struct_def

//                         #rt_action

//                         Ok(#display_res)
//                     }
//                 }
//             } else {
//                 let invocation = if opt.is_async {
//                     quote! {
//                         rt_ref.block_on(#fn_path(#(#param),*))?
//                     }
//                 } else {
//                     quote! {
//                         #fn_path(#(#param),*)?
//                     }
//                 };
//                 let display_res = if let Some(display_fn) = &opt.display {
//                     quote! {
//                         #display_fn(#invocation, matches)
//                     }
//                 } else {
//                     quote! {
//                         ::rpc_toolkit::command_helpers::prelude::default_display(#invocation, matches)
//                     }
//                 };
//                 let rt_action = if opt.is_async {
//                     create_rt
//                 } else {
//                     quote! {
//                         drop(rt);
//                     }
//                 };
//                 quote! {
//                     pub fn cli_handler#generics(
//                         ctx: GenericContext,
//                         parent_data: #parent_data_ty,
//                         mut rt: Option<::rpc_toolkit::command_helpers::prelude::Runtime>,
//                         matches: &::rpc_toolkit::command_helpers::prelude::ArgMatches,
//                         _method: ::rpc_toolkit::command_helpers::prelude::Cow<'_, str>,
//                         parent_params: ParentParams
//                     ) -> Result<(), ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                         #param_struct_def

//                         #rt_action

//                         Ok(#display_res)
//                     }
//                 }
//             }
//         }
//         Options::Parent(ParentOptions {
//             common,
//             subcommands,
//             self_impl,
//         }) => {
//             let cmd_preprocess = if common.is_async {
//                 quote! {
//                     #create_rt
//                     let parent_data = rt_ref.block_on(#fn_path(#(#param),*))?;
//                 }
//             } else {
//                 quote! {
//                     let parent_data = #fn_path(#(#param),*)?;
//                 }
//             };
//             let subcmd_impl = subcommands.iter().map(|subcommand| {
//                 let mut subcommand = subcommand.clone();
//                 let mut cli_handler = PathSegment {
//                     ident: Ident::new("cli_handler", Span::call_site()),
//                     arguments: std::mem::replace(
//                         &mut subcommand.segments.last_mut().unwrap().arguments,
//                         PathArguments::None,
//                     ),
//                 };
//                 cli_handler.arguments = match cli_handler.arguments {
//                     PathArguments::None => PathArguments::AngleBracketed(
//                         syn::parse2(quote! { ::<Params#param_ty_generics, GenericContext> })
//                             .unwrap(),
//                     ),
//                     PathArguments::AngleBracketed(mut a) => {
//                         a.args
//                             .push(syn::parse2(quote! { Params#param_ty_generics }).unwrap());
//                         a.args.push(syn::parse2(quote! { GenericContext }).unwrap());
//                         PathArguments::AngleBracketed(a)
//                     }
//                     _ => unreachable!(),
//                 };
//                 quote_spanned! { subcommand.span() =>
//                     Some((#subcommand::NAME, sub_m)) => {
//                         let method = if method.is_empty() {
//                             #subcommand::NAME.into()
//                         } else {
//                             method + "." + #subcommand::NAME
//                         };
//                         #subcommand::#cli_handler(ctx, parent_data, rt, sub_m, method, params)
//                     },
//                 }
//             });
//             let self_impl = match (self_impl, &common.exec_ctx) {
//                 (Some(self_impl), ExecutionContext::CliOnly(_))
//                 | (Some(self_impl), ExecutionContext::Local(_))
//                 | (Some(self_impl), ExecutionContext::CustomCli { .. }) => {
//                     let (self_impl_fn, is_async) =
//                         if let ExecutionContext::CustomCli { cli, is_async, .. } = &common.exec_ctx
//                         {
//                             (cli, *is_async)
//                         } else {
//                             (&self_impl.path, self_impl.is_async)
//                         };
//                     let create_rt = if common.is_async {
//                         None
//                     } else {
//                         Some(create_rt)
//                     };
//                     let self_impl = if is_async {
//                         quote_spanned! { self_impl_fn.span() =>
//                             #create_rt
//                             rt_ref.block_on(#self_impl_fn(Into::into(ctx), parent_data))?
//                         }
//                     } else {
//                         quote_spanned! { self_impl_fn.span() =>
//                             #self_impl_fn(Into::into(ctx), parent_data)?
//                         }
//                     };
//                     quote! {
//                         Ok(#display(#self_impl, matches)),
//                     }
//                 }
//                 (Some(self_impl), ExecutionContext::Standard) => {
//                     let self_impl_fn = &self_impl.path;
//                     let self_impl = if self_impl.is_async {
//                         quote! {
//                             rt_ref.block_on(#self_impl_fn(unreachable!(), parent_data))
//                         }
//                     } else {
//                         quote! {
//                             #self_impl_fn(unreachable!(), parent_data)
//                         }
//                     };
//                     let create_rt = if common.is_async {
//                         None
//                     } else {
//                         Some(create_rt)
//                     };
//                     quote! {
//                         {
//                             #create_rt

//                             #[allow(unreachable_code)]
//                             let return_ty = if true {
//                                 ::rpc_toolkit::command_helpers::prelude::PhantomData
//                             } else {
//                                 ::rpc_toolkit::command_helpers::prelude::make_phantom(#self_impl?)
//                             };

//                             let res = rt_ref.block_on(::rpc_toolkit::command_helpers::prelude::call_remote(ctx, method.as_ref(), params, return_ty))?;
//                             Ok(#display(res.result?, matches))
//                         }
//                     }
//                 }
//                 (None, _) | (Some(_), ExecutionContext::RpcOnly(_)) => quote! {
//                     Err(::rpc_toolkit::command_helpers::prelude::RpcError {
//                         data: Some(method.into()),
//                         ..::rpc_toolkit::command_helpers::prelude::yajrc::METHOD_NOT_FOUND_ERROR
//                     }),
//                 },
//             };
//             quote! {
//                 pub fn cli_handler#generics(
//                     ctx: GenericContext,
//                     parent_data: #parent_data_ty,
//                     mut rt: Option<::rpc_toolkit::command_helpers::prelude::Runtime>,
//                     matches: &::rpc_toolkit::command_helpers::prelude::ArgMatches,
//                     method: ::rpc_toolkit::command_helpers::prelude::Cow<'_, str>,
//                     parent_params: ParentParams,
//                 ) -> Result<(), ::rpc_toolkit::command_helpers::prelude::RpcError> {
//                     #param_struct_def

//                     #cmd_preprocess

//                     match matches.subcommand() {
//                         #(
//                             #subcmd_impl
//                         )*
//                         _ => #self_impl
//                     }
//                 }
//             }
//         }
//     }
// }

fn build_params(params: Vec<ArgOptions>, generics: &Generics) -> (TokenStream, Generics) {
    let mut param_generics_filter = GenericFilter::new(generics);
    for param in &params {
        param_generics_filter.fold_type(param.ty.clone());
    }
    let param_generics = param_generics_filter.finish();
    let (impl_generics, ty_generics, where_clause) = param_generics.split_for_impl();
    let param_arg = params.iter().enumerate().map(|(idx, p)| {
        let mut res = TokenStream::new();
        let p_ty = &p.ty;
        if let Some(rename) = &p.rename {
            res.extend(quote! { #[serde(rename = #rename)] });
        } else if let Some(name) = &p.name {
            let name = LitStr::new(&name.to_string(), name.span());
            res.extend(quote! { #[serde(rename = #name)] });
        };
        if let Some(default) = &p.default {
            res.extend(quote! { #[serde(#default)] });
        }
        let arg_ident = Ident::new(&format!("arg_{idx}"), Span::call_site());
        let p_name = p.name.as_ref().unwrap_or(&arg_ident);
        res.extend(quote! { pub #p_name: #p_ty, });
        res
    });
    let (clap_param_arg, clap_param_from_matches): (Vec<_>, Vec<_>) = params
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let (mut arg, mut from_matches) = (TokenStream::new(), TokenStream::new());
            let arg_ident = Ident::new(&format!("arg_{idx}"), Span::call_site());
            let p_name = p
                .name
                .as_ref()
                .unwrap_or(&arg_ident);
            if p.stdin.is_some() {
                let parser = p
                    .parse.as_ref()
                    .map(|p| quote! { #p })
                    .unwrap_or(quote!(::rpc_toolkit::command_helpers::default_stdin_parser));
                from_matches.extend(quote! { #p_name: #parser(&mut std::io::stdin(), matches)? });
            } else if matches!(&p.ty, Type::Path(p) if p.path.is_ident("bool")) {
                arg.extend(if p.clap_attr.is_empty() {
                    quote! { #[arg] }
                } else {
                    let clap_attr = &p.clap_attr;
                    quote! { #[arg(#(#clap_attr),*)] }
                });
                arg.extend(quote! { #p_name: bool, });
                from_matches.extend(quote! { #p_name: clap_args.#p_name, });
            } else if matches!(&p.ty, Type::Path(p) if p.path.segments.first().unwrap().ident == "Option") {
                let parser = p
                    .parse.as_ref()
                    .map(|p| quote!(#p))
                    .unwrap_or(quote!(::rpc_toolkit::command_helpers::default_arg_parser));
                arg.extend(if p.clap_attr.is_empty() {
                    quote! { #[arg] }
                } else {
                    let clap_attr = &p.clap_attr;
                    quote! { #[arg(#(#clap_attr),*)] }
                });
                arg.extend(quote! { #p_name: Option<String>, });
                from_matches.extend(quote! { #p_name: clap_args.#p_name.as_ref().map(|arg_str| #parser(arg_str, matches)).transpose()?, });
            } else {
                let parser = p
                    .parse.as_ref()
                    .map(|p| quote!(#p))
                    .unwrap_or(quote!(::rpc_toolkit::command_helpers::default_arg_parser));
                arg.extend(if p.clap_attr.is_empty() {
                    quote! { #[arg] }
                } else {
                    let clap_attr = &p.clap_attr;
                    quote! { #[arg(#(#clap_attr),*)] }
                });
                arg.extend(quote! { #p_name: String, });
                from_matches.extend(quote! { #p_name: #parser(&clap_args.#p_name, matches)?, });
            }
            (arg, from_matches)
        })
        .unzip();
    (
        quote! {
            #[derive(::rpc_toolkit::serde::Serialize, ::rpc_toolkit::serde::Deserialize)]
            pub struct Params #ty_generics {
                #(
                    #param_arg
                )*
            }
            #[derive(::rpc_toolkit::clap::Parser)]
            struct ClapParams {
                #(
                    #clap_param_arg
                )*
            }
            impl #impl_generics ::rpc_toolkit::command_helpers::clap::FromArgMatches for Params #ty_generics #where_clause {
                fn from_arg_matches(matches: &::rpc_toolkit::command_helpers::clap::ArgMatches) -> Result<Self, ::rpc_toolkit::command_helpers::clap::Error> {
                    let clap_args = ClapParams::from_arg_matches(matches)?;
                    Ok(Self {
                        #(
                            #clap_param_from_matches
                        )*
                    })
                }
                fn update_from_arg_matches(&mut self, matches: &::rpc_toolkit::command_helpers::clap::ArgMatches) -> Result<(), ::rpc_toolkit::command_helpers::clap::Error> {
                    unimplemented!()
                }
            }
            impl #impl_generics ::rpc_toolkit::command_helpers::clap::CommandFactory for Params #ty_generics #where_clause {
                fn command() -> ::rpc_toolkit::command_helpers::clap::Command {
                    ClapParams::command()
                }
                fn command_for_update() -> ::rpc_toolkit::command_helpers::clap::Command {
                    ClapParams::command_for_update()
                }
            }
        },
        param_generics,
    )
}

fn build_inherited(parent_data: Option<Type>, generics: &Generics) -> (TokenStream, Generics) {
    let mut inherited_generics_filter = GenericFilter::new(generics);
    if let Some(inherited) = &parent_data {
        inherited_generics_filter.fold_type(inherited.clone());
    }
    let inherited_generics = inherited_generics_filter.finish();
    if let Some(inherited) = parent_data {
        let (_, ty_generics, _) = inherited_generics.split_for_impl();
        (
            quote! {
                type InheritedParams #ty_generics = #inherited;
            },
            inherited_generics,
        )
    } else {
        (
            quote! { type InheritedParams = ::rpc_toolkit::Empty; },
            inherited_generics,
        )
    }
}

pub fn build(args: AttributeArgs, mut item: ItemFn) -> TokenStream {
    let params = macro_try!(parse_param_attrs(&mut item));
    let mut opt = macro_try!(parse_command_attr(args));
    let fn_vis = &item.vis;
    let fn_name = &item.sig.ident;
    let (params_impl, params_generics) = build_params(
        params
            .iter()
            .filter_map(|a| {
                if let ParamType::Arg(a) = a {
                    Some(a.clone())
                } else {
                    None
                }
            })
            .collect(),
        &item.sig.generics,
    );
    let (_, params_ty_generics, _) = params_generics.split_for_impl();
    let (inherited_impl, inherited_generics) = build_inherited(
        params.iter().find_map(|a| {
            if let ParamType::ParentData(a) = a {
                Some(a.clone())
            } else {
                None
            }
        }),
        &item.sig.generics,
    );
    let (_, inherited_ty_generics, _) = inherited_generics.split_for_impl();
    let mut params_and_inherited_generics_filter = GenericFilter::new(&item.sig.generics);
    params_and_inherited_generics_filter.fold_generics(params_generics.clone());
    params_and_inherited_generics_filter.fold_generics(inherited_generics.clone());
    let params_and_inherited_generics = params_and_inherited_generics_filter.finish();
    let (_, params_and_inherited_ty_generics, _) = params_and_inherited_generics.split_for_impl();
    let phantom_ty = params_and_inherited_generics
        .type_params()
        .map(|t| &t.ident)
        .map(|t| quote! { #t })
        .chain(
            params_and_inherited_generics
                .lifetimes()
                .map(|l| &l.lifetime)
                .map(|l| quote! { &#l () }),
        );
    let phantom_ty = quote! { ( #( #phantom_ty, )* ) };
    let module = if let Options::Parent(parent) = &opt {
        // let subcommands = parent.subcommands.iter().map(|c| quote! { .subcommand_with_inherited_remote_cli(c::handler(), |a, b| ) })
        // quote! {
        //     pub type Handler #params_and_inherited_ty_generics = ::rpc_toolkit::ParentHandler<Params #params_generics, InheritedParams #inherited_generics>;
        //     #params_impl
        //     #inherited_impl
        //     pub fn handler #params_and_inherited_ty_generics () -> Handler #params_and_inherited_ty_generics {
        //         Handler::new()
        //             #(
        //                 #subcommands
        //             )*
        //     }
        // }
        Error::new(Span::call_site(), "derived parent handlers not implemented").to_compile_error()
    } else {
        let (ok_ty, err_ty) = match &item.sig.output {
            ReturnType::Type(_, p) => match &**p {
                Type::Path(p) if p.path.segments.last().unwrap().ident == "Result" => {
                    match &p.path.segments.last().unwrap().arguments {
                        PathArguments::AngleBracketed(a) if a.args.len() == 2 => {
                            match (a.args.first(), a.args.last()) {
                                (
                                    Some(GenericArgument::Type(ok)),
                                    Some(GenericArgument::Type(err)),
                                ) => (ok, err),
                                _ => {
                                    return Error::new(a.span(), "return type must be a Result")
                                        .to_compile_error()
                                }
                            }
                        }
                        a => {
                            return Error::new(a.span(), "return type must be a Result")
                                .to_compile_error()
                        }
                    }
                }
                a => {
                    return Error::new(a.span(), "return type must be a Result").to_compile_error();
                }
            },
            a => return Error::new(a.span(), "return type must be a Result").to_compile_error(),
        };
        let handler_impl: TokenStream = todo!();
        let cli_bindings_impl: TokenStream = todo!();
        quote! {
            pub struct Handler #params_and_inherited_ty_generics (::core::marker::PhantomData<#phantom_ty>);
            impl #params_and_inherited_ty_generics ::core::fmt::Debug for Handler #params_and_inherited_ty_generics {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    f.debug_tuple("Handler")
                        .finish()
                }
            }
            impl #params_and_inherited_ty_generics ::core::clone::Clone for Handler #params_and_inherited_ty_generics {
                fn clone(&self) -> Self {
                    Self(::core::marker::PhantomData)
                }
            }
            impl #params_and_inherited_ty_generics ::core::marker::Copy for Handler #params_and_inherited_ty_generics { }
            #params_impl
            #inherited_impl
            impl #params_and_inherited_ty_generics ::rpc_toolkit::HandlerTypes for Handler #params_and_inherited_ty_generics {
                type Params = Params;
                type InheritedParams = InheritedParams;
                type Ok = #ok_ty;
                type Err = #err_ty;
            }
            #handler_impl
            #cli_bindings_impl
            pub fn handler #params_and_inherited_ty_generics () -> Handler #params_and_inherited_ty_generics {
                Handler(::core::marker::PhantomData)
            }
        }
    };

    let fn_rename = opt
        .common()
        .rename
        .clone()
        .unwrap_or(LitStr::new(&fn_name.to_string(), fn_name.span()));

    let res = quote! {
        #item
        #fn_vis mod #fn_name {
            use super::*;
            pub const NAME: &str = #fn_rename;
            #module
        }
    };
    if let Some(debug) = &opt.common().macro_debug {
        Error::new(debug.span(), res).into_compile_error()
    } else {
        res
    }
}
