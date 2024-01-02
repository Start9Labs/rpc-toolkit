use std::any::TypeId;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches};
use imbl_value::imbl::{OrdMap, OrdSet};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{combine, Flat};
use crate::{
    AnyContext, AnyHandler, CliBindings, DynHandler, HandleAny, HandleAnyArgs, HandleArgs, Handler,
    HandlerTypes, IntoContext, NoCli, NoParams,
};

pub trait IntoHandlers: HandlerTypes {
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler)>;
}

impl<H: Handler + CliBindings> IntoHandlers for H {
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler)> {
        iter_from_ctx_and_handler(
            intersect_type_ids(self.contexts(), <Self as CliBindings>::Context::type_ids()),
            DynHandler::WithCli(Arc::new(AnyHandler::new(self))),
        )
    }
}

pub(crate) fn iter_from_ctx_and_handler(
    ctx: Option<OrdSet<TypeId>>,
    handler: DynHandler,
) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler)> {
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

#[derive(Debug, Clone)]
pub(crate) struct SubcommandMap(pub(crate) OrdMap<Name, OrdMap<Option<TypeId>, DynHandler>>);
impl SubcommandMap {
    fn insert(
        &mut self,
        name: Option<&'static str>,
        handlers: impl IntoIterator<Item = (Option<TypeId>, DynHandler)>,
    ) {
        let mut for_name = self.0.remove(&name).unwrap_or_default();
        for (ctx_ty, handler) in handlers {
            for_name.insert(ctx_ty, handler);
        }
        self.0.insert(Name(name), for_name);
    }

    fn get<'a>(&'a self, ctx_ty: TypeId, name: Option<&str>) -> Option<(Name, &'a DynHandler)> {
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

pub struct ParentHandler<Params = NoParams, InheritedParams = NoParams> {
    _phantom: PhantomData<(Params, InheritedParams)>,
    pub(crate) subcommands: SubcommandMap,
    metadata: OrdMap<&'static str, Value>,
}
impl<Params, InheritedParams> ParentHandler<Params, InheritedParams> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
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
            _phantom: PhantomData,
            subcommands: self.subcommands.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
impl<Params, InheritedParams> std::fmt::Debug for ParentHandler<Params, InheritedParams> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ParentHandler")
            .field(&self.subcommands)
            .finish()
    }
}

struct InheritanceHandler<Params, InheritedParams, H, F> {
    _phantom: PhantomData<(Params, InheritedParams)>,
    handler: H,
    inherit: F,
}
impl<Params, InheritedParams, H: Clone, F: Clone> Clone
    for InheritanceHandler<Params, InheritedParams, H, F>
{
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            handler: self.handler.clone(),
            inherit: self.inherit.clone(),
        }
    }
}
impl<Params, InheritedParams, H: std::fmt::Debug, F> std::fmt::Debug
    for InheritanceHandler<Params, InheritedParams, H, F>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("InheritanceHandler")
            .field(&self.handler)
            .finish()
    }
}
impl<Params, InheritedParams, H, F> HandlerTypes
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    H: HandlerTypes,
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = H::Params;
    type InheritedParams = Flat<Params, InheritedParams>;
    type Ok = H::Ok;
    type Err = H::Err;
}
#[async_trait::async_trait]
impl<Params, InheritedParams, H, F> Handler for InheritanceHandler<Params, InheritedParams, H, F>
where
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: Handler,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
{
    type Context = H::Context;
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
            raw_params,
        })
    }
    async fn handle_async(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
            raw_params,
        })
    }
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method, ctx_ty)
    }
    fn contexts(&self) -> Option<OrdSet<TypeId>> {
        self.handler.contexts()
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method, ctx_ty)
    }
}

impl<Params, InheritedParams, H, F> CliBindings
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: CliBindings,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
{
    type Context = H::Context;
    fn cli_command(&self, ctx_ty: TypeId) -> Command {
        self.handler.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches, ctx_ty)
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.cli_display(
            HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
                raw_params,
            },
            result,
        )
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
    pub fn subcommand<H>(mut self, name: &'static str, handler: H) -> Self
    where
        H: IntoHandlers<InheritedParams = Flat<Params, InheritedParams>>,
    {
        self.subcommands
            .insert(name.into(), handler.into_handlers());
        self
    }
    pub fn root_handler<H>(mut self, handler: H) -> Self
    where
        H: IntoHandlers<Params = NoParams, InheritedParams = Flat<Params, InheritedParams>>,
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
    Params: Serialize + Send + Sync + 'static,
    InheritedParams: Serialize + Send + Sync + 'static,
{
    type Context = AnyContext;
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            raw_params,
            ..
        }: HandleArgs<AnyContext, Self>,
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
            })
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    async fn handle_async(
        &self,
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            raw_params,
            ..
        }: HandleArgs<AnyContext, Self>,
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
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            raw_params,
            ..
        }: HandleArgs<AnyContext, Self>,
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
                },
                result,
            )
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
}
impl<Params, InheritedParams> IntoHandlers for ParentHandler<Params, InheritedParams>
where
    Self: HandlerTypes + Handler + CliBindings,
    <Self as HandlerTypes>::Params: DeserializeOwned,
    <Self as HandlerTypes>::InheritedParams: DeserializeOwned,
    <Self as HandlerTypes>::Ok: Serialize + DeserializeOwned,
    RpcError: From<<Self as HandlerTypes>::Err>,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler)> {
        iter_from_ctx_and_handler(
            self.contexts(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(self))),
        )
    }
}
impl<Params, InheritedParams> IntoHandlers for NoCli<ParentHandler<Params, InheritedParams>>
where
    ParentHandler<Params, InheritedParams>: HandlerTypes + Handler,
    <ParentHandler<Params, InheritedParams> as HandlerTypes>::Params: DeserializeOwned,
    <ParentHandler<Params, InheritedParams> as HandlerTypes>::InheritedParams: DeserializeOwned,
    <ParentHandler<Params, InheritedParams> as HandlerTypes>::Ok: Serialize + DeserializeOwned,
    RpcError: From<<ParentHandler<Params, InheritedParams> as HandlerTypes>::Err>,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler)> {
        iter_from_ctx_and_handler(
            self.0.contexts(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(self.0))),
        )
    }
}
