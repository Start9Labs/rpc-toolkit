use std::any::TypeId;
use std::collections::VecDeque;
use std::sync::Arc;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches};
use imbl_value::imbl::{OrdMap, OrdSet};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{combine, Flat, PhantomData};
use crate::{
    AnyContext, AnyHandler, CliBindings, DynHandler, Empty, HandleAny, HandleAnyArgs, Handler,
    HandlerArgs, HandlerArgsFor, HandlerExt, HandlerTypes, IntoContext, OrEmpty,
};

pub trait IntoHandlers<Inherited>: HandlerTypes {
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler<Inherited>)>;
}

impl<H, A, B> IntoHandlers<Flat<A, B>> for H
where
    H: Handler + CliBindings,
    H::Params: DeserializeOwned,
    H::InheritedParams: OrEmpty<Flat<A, B>>,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
    A: Send + Sync + 'static,
    B: Send + Sync + 'static,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler<Flat<A, B>>)> {
        iter_from_ctx_and_handler(
            intersect_type_ids(self.contexts(), <Self as CliBindings>::Context::type_ids()),
            DynHandler::WithCli(Arc::new(AnyHandler::new(
                self.with_inherited(|a, b| OrEmpty::from_t(Flat(a, b))),
            ))),
        )
    }
}

pub(crate) fn iter_from_ctx_and_handler<Inherited>(
    ctx: Option<OrdSet<TypeId>>,
    handler: DynHandler<Inherited>,
) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler<Inherited>)> {
    if let Some(ctx) = ctx {
        itertools::Either::Left(ctx.into_iter().map(Some))
    } else {
        itertools::Either::Right(std::iter::once(None))
    }
    .map(move |ctx| (ctx, handler.clone()))
}

pub(crate) fn intersect_type_ids(
    a: Option<OrdSet<TypeId>>,
    b: Option<OrdSet<TypeId>>,
) -> Option<OrdSet<TypeId>> {
    match (a, b) {
        (None, None) => None,
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (Some(a), Some(b)) => Some(a.intersection(b)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Name(pub(crate) Option<&'static str>);
impl<'a> std::borrow::Borrow<Option<&'a str>> for Name {
    fn borrow(&self) -> &Option<&'a str> {
        &self.0
    }
}

pub(crate) struct SubcommandMap<Inherited>(
    pub(crate) OrdMap<Name, OrdMap<Option<TypeId>, DynHandler<Inherited>>>,
);
impl<Inherited> Clone for SubcommandMap<Inherited> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<Inherited> SubcommandMap<Inherited> {
    fn insert(
        &mut self,
        name: Option<&'static str>,
        handlers: impl IntoIterator<Item = (Option<TypeId>, DynHandler<Inherited>)>,
    ) {
        let mut for_name = self.0.remove(&name).unwrap_or_default();
        for (ctx_ty, handler) in handlers {
            for_name.insert(ctx_ty, handler);
        }
        self.0.insert(Name(name), for_name);
    }

    fn get<'a>(
        &'a self,
        ctx_ty: TypeId,
        name: Option<&str>,
    ) -> Option<(Name, &'a DynHandler<Inherited>)> {
        if let Some((name, for_name)) = self.0.get_key_value(&name) {
            if let Some(for_ctx) = for_name.get(&Some(ctx_ty)) {
                Some((*name, for_ctx))
            } else {
                for_name.get(&None).map(|h| (*name, h))
            }
        } else {
            None
        }
    }
}

pub struct ParentHandler<Params = Empty, InheritedParams = Empty> {
    _phantom: PhantomData<Params>,
    pub(crate) subcommands: SubcommandMap<Flat<Params, InheritedParams>>,
    metadata: OrdMap<&'static str, Value>,
}
impl<Params, InheritedParams> ParentHandler<Params, InheritedParams> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData::new(),
            subcommands: SubcommandMap(OrdMap::new()),
            metadata: OrdMap::new(),
        }
    }
    pub fn with_metadata(mut self, key: &'static str, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
impl<Params, InheritedParams> Clone for ParentHandler<Params, InheritedParams> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            subcommands: self.subcommands.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
impl<Params, InheritedParams> std::fmt::Debug for ParentHandler<Params, InheritedParams> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ParentHandler")
            // .field(&self.subcommands)
            .finish()
    }
}

impl<Params, InheritedParams> ParentHandler<Params, InheritedParams> {
    fn get_contexts(&self) -> Option<OrdSet<TypeId>> {
        let mut set = OrdSet::new();
        for ctx_ty in self.subcommands.0.values().flat_map(|c| c.keys()) {
            set.insert((*ctx_ty)?);
        }
        Some(set)
    }
    #[allow(private_bounds)]
    pub fn subcommand<H>(mut self, name: &'static str, handler: H) -> Self
    where
        H: IntoHandlers<Flat<Params, InheritedParams>>,
    {
        self.subcommands
            .insert(name.into(), handler.into_handlers());
        self
    }
    #[allow(private_bounds)]
    pub fn root_handler<H>(mut self, handler: H) -> Self
    where
        H: IntoHandlers<Flat<Params, InheritedParams>>,
    {
        self.subcommands.insert(None, handler.into_handlers());
        self
    }
}

impl<Params, InheritedParams> HandlerTypes for ParentHandler<Params, InheritedParams>
where
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = Value;
    type Err = RpcError;
}
#[async_trait::async_trait]
impl<Params, InheritedParams> Handler for ParentHandler<Params, InheritedParams>
where
    Params: Send + Sync + 'static,
    InheritedParams: Serialize + Send + Sync + 'static,
{
    type Context = AnyContext;
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<AnyContext, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
        }
        if let Some((_, sub_handler)) = &self.subcommands.get(context.inner_type_id(), cmd) {
            sub_handler.handle_sync(HandleAnyArgs {
                context,
                parent_method,
                method,
                params: raw_params,
                inherited: Flat(params, inherited_params),
            })
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    async fn handle_async(
        &self,
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<AnyContext, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
        }
        if let Some((_, sub_handler)) = self.subcommands.get(context.inner_type_id(), cmd) {
            sub_handler
                .handle_async(HandleAnyArgs {
                    context,
                    parent_method,
                    method,
                    params: raw_params,
                    inherited: Flat(params, inherited_params),
                })
                .await
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    fn metadata(
        &self,
        mut method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        let metadata = self.metadata.clone();
        if let Some((_, handler)) = self.subcommands.get(ctx_ty, method.pop_front()) {
            handler.metadata(method, ctx_ty).union(metadata)
        } else {
            metadata
        }
    }
    fn contexts(&self) -> Option<OrdSet<TypeId>> {
        self.get_contexts()
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        let (head, tail) = if method.is_empty() {
            (None, None)
        } else {
            method
                .split_once(".")
                .map(|(head, tail)| (Some(head), Some(tail)))
                .unwrap_or((Some(method), None))
        };
        let (Name(name), h) = self.subcommands.get(ctx_ty, head)?;
        let mut res = VecDeque::new();
        if let Some(name) = name {
            res.push_back(name);
        }
        if let Some(tail) = tail {
            res.append(&mut h.method_from_dots(tail, ctx_ty)?);
        }
        Some(res)
    }
}

impl<Params, InheritedParams> CliBindings for ParentHandler<Params, InheritedParams>
where
    Params: FromArgMatches + CommandFactory + Serialize + Send + Sync + 'static,
    InheritedParams: Serialize + Send + Sync + 'static,
{
    type Context = AnyContext;
    fn cli_command(&self, ctx_ty: TypeId) -> Command {
        let mut base = Params::command().subcommand_required(true);
        for (name, handlers) in &self.subcommands.0 {
            match (
                name,
                if let Some(handler) = handlers.get(&Some(ctx_ty)) {
                    Some(handler)
                } else if let Some(handler) = handlers.get(&None) {
                    Some(handler)
                } else {
                    None
                },
            ) {
                (Name(Some(name)), Some(DynHandler::WithCli(handler))) => {
                    base = base.subcommand(handler.cli_command(ctx_ty).name(name));
                }
                (Name(None), Some(DynHandler::WithCli(_))) => {
                    base = base.subcommand_required(false);
                }
                _ => (),
            }
        }
        base
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        let root_params = imbl_value::to_value(&Params::from_arg_matches(matches)?)
            .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?;
        let (name, matches) = match matches.subcommand() {
            Some((name, matches)) => (Some(name), matches),
            None => (None, matches),
        };
        if let Some((Name(Some(name)), DynHandler::WithCli(handler))) =
            self.subcommands.get(ctx_ty, name)
        {
            let (mut method, params) = handler.cli_parse(matches, ctx_ty)?;
            method.push_front(name);

            Ok((
                method,
                combine(root_params, params)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ArgumentConflict, e))?,
            ))
        } else {
            Ok((VecDeque::new(), root_params))
        }
    }
    fn cli_display(
        &self,
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<AnyContext, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
        }
        if let Some((_, DynHandler::WithCli(sub_handler))) =
            self.subcommands.get(context.inner_type_id(), cmd)
        {
            sub_handler.cli_display(
                HandleAnyArgs {
                    context,
                    parent_method,
                    method,
                    params: raw_params,
                    inherited: Flat(params, inherited_params),
                },
                result,
            )
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
}
