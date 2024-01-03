use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use clap::{CommandFactory, FromArgMatches};
use imbl_value::imbl::{OrdMap, OrdSet};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{internal_error, parse_error, Flat};
use crate::{
    iter_from_ctx_and_handler, AnyContext, AnyHandler, CallRemote, CliBindings, DynHandler,
    EitherContext, HandleArgs, Handler, HandlerTypes, IntoContext, IntoHandlers, PrintCliResult,
};

pub trait HandlerExt: Handler + Sized {
    fn no_cli(self) -> NoCli<Self>;
    fn no_display(self) -> NoDisplay<Self>;
    fn with_custom_display<P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            Params = Self::Params,
            InheritedParams = Self::InheritedParams,
            Ok = Self::Ok,
            Err = Self::Err,
        >;
    fn with_custom_display_fn<Context: IntoContext, F>(
        self,
        display: F,
    ) -> CustomDisplayFn<Context, F, Self>
    where
        F: Fn(HandleArgs<Context, Self>, Self::Ok) -> Result<(), Self::Err>;
    fn with_inherited<Params, InheritedParams, F>(
        self,
        f: F,
    ) -> InheritanceHandler<Params, InheritedParams, Self, F>
    where
        F: Fn(Params, InheritedParams) -> Self::InheritedParams;
    fn with_remote_cli<Context>(self) -> RemoteCli<Context, Self>;
}

impl<T: Handler + Sized> HandlerExt for T {
    fn no_cli(self) -> NoCli<Self> {
        NoCli(self)
    }
    fn no_display(self) -> NoDisplay<Self> {
        NoDisplay(self)
    }
    fn with_custom_display<P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            Params = Self::Params,
            InheritedParams = Self::InheritedParams,
            Ok = Self::Ok,
            Err = Self::Err,
        >,
    {
        CustomDisplay {
            print: display,
            handler: self,
        }
    }
    fn with_custom_display_fn<Context: IntoContext, F>(
        self,
        display: F,
    ) -> CustomDisplayFn<Context, F, Self>
    where
        F: Fn(HandleArgs<Context, Self>, Self::Ok) -> Result<(), Self::Err>,
    {
        CustomDisplayFn {
            _phantom: PhantomData,
            print: display,
            handler: self,
        }
    }
    fn with_inherited<Params, InheritedParams, F>(
        self,
        f: F,
    ) -> InheritanceHandler<Params, InheritedParams, Self, F>
    where
        F: Fn(Params, InheritedParams) -> Self::InheritedParams,
    {
        InheritanceHandler {
            _phantom: PhantomData,
            handler: self,
            inherit: f,
        }
    }
    fn with_remote_cli<Context>(self) -> RemoteCli<Context, Self> {
        RemoteCli {
            _phantom: PhantomData,
            handler: self,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoCli<H>(pub H);
impl<H: HandlerTypes> HandlerTypes for NoCli<H> {
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}
impl<H> IntoHandlers for NoCli<H>
where
    H: Handler,
    H::Params: DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler)> {
        iter_from_ctx_and_handler(
            self.0.contexts(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(self.0))),
        )
    }
}

#[derive(Debug, Clone)]
pub struct NoDisplay<H>(pub H);
impl<H: HandlerTypes> HandlerTypes for NoDisplay<H> {
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}
#[async_trait::async_trait]
impl<H> Handler for NoDisplay<H>
where
    H: Handler,
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
        self.0.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
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
        self.0
            .handle_async(HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            })
            .await
    }
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        self.0.metadata(method, ctx_ty)
    }
    fn contexts(&self) -> Option<OrdSet<TypeId>> {
        self.0.contexts()
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.0.method_from_dots(method, ctx_ty)
    }
}
impl<H> CliBindings for NoDisplay<H>
where
    H: HandlerTypes,
    H::Params: FromArgMatches + CommandFactory + Serialize,
{
    type Context = AnyContext;
    fn cli_command(&self, _: TypeId) -> clap::Command {
        H::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
        _: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        Self::Params::from_arg_matches(matches).and_then(|a| {
            Ok((
                VecDeque::new(),
                imbl_value::to_value(&a)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?,
            ))
        })
    }
    fn cli_display(
        &self,
        _: HandleArgs<Self::Context, Self>,
        _: Self::Ok,
    ) -> Result<(), Self::Err> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CustomDisplay<P, H> {
    print: P,
    handler: H,
}
impl<P, H> HandlerTypes for CustomDisplay<P, H>
where
    H: HandlerTypes,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}
#[async_trait::async_trait]
impl<P, H> Handler for CustomDisplay<P, H>
where
    H: Handler,
    P: Send + Sync + Clone + Debug + 'static,
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
            inherited_params,
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
        self.handler
            .handle_async(HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            })
            .await
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
impl<P, H> CliBindings for CustomDisplay<P, H>
where
    H: HandlerTypes,
    H::Params: FromArgMatches + CommandFactory + Serialize,
    P: PrintCliResult<
            Params = H::Params,
            InheritedParams = H::InheritedParams,
            Ok = H::Ok,
            Err = H::Err,
        > + Send
        + Sync
        + Clone
        + Debug
        + 'static,
{
    type Context = P::Context;
    fn cli_command(&self, _: TypeId) -> clap::Command {
        H::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
        _: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        Self::Params::from_arg_matches(matches).and_then(|a| {
            Ok((
                VecDeque::new(),
                imbl_value::to_value(&a)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?,
            ))
        })
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
        self.print.print(
            HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}

pub struct CustomDisplayFn<Context, F, H> {
    _phantom: PhantomData<Context>,
    print: F,
    handler: H,
}
impl<Context, F: Clone, H: Clone> Clone for CustomDisplayFn<Context, F, H> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            print: self.print.clone(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, F: Debug, H: Debug> Debug for CustomDisplayFn<Context, F, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomDisplayFn")
            .field("print", &self.print)
            .field("handler", &self.handler)
            .finish()
    }
}
impl<Context, F, H> HandlerTypes for CustomDisplayFn<Context, F, H>
where
    H: HandlerTypes,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}
#[async_trait::async_trait]
impl<Context, F, H> Handler for CustomDisplayFn<Context, F, H>
where
    Context: Send + Sync + 'static,
    H: Handler,
    F: Send + Sync + Clone + Debug + 'static,
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
            inherited_params,
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
        self.handler
            .handle_async(HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            })
            .await
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
impl<Context, F, H> CliBindings for CustomDisplayFn<Context, F, H>
where
    Context: IntoContext,
    H: CliBindings,
    F: Fn(HandleArgs<Context, H>, H::Ok) -> Result<(), H::Err>
        + Send
        + Sync
        + Clone
        + Debug
        + 'static,
{
    type Context = Context;
    fn cli_command(&self, ctx_ty: TypeId) -> clap::Command {
        self.handler.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
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
        }: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        (self.print)(
            HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}

pub struct RemoteCli<Context, H> {
    _phantom: PhantomData<Context>,
    handler: H,
}
impl<Context, H: Clone> Clone for RemoteCli<Context, H> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            handler: self.handler.clone(),
        }
    }
}
impl<Context, H: Debug> Debug for RemoteCli<Context, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RemoteCli").field(&self.handler).finish()
    }
}
impl<Context, H> HandlerTypes for RemoteCli<Context, H>
where
    H: HandlerTypes,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}
#[async_trait::async_trait]
impl<Context, H> Handler for RemoteCli<Context, H>
where
    Context: CallRemote,
    H: Handler,
    H::Params: Serialize,
    H::InheritedParams: Serialize,
    H::Ok: DeserializeOwned,
    H::Err: From<RpcError>,
{
    type Context = EitherContext<Context, H::Context>;
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
        match context {
            EitherContext::C1(context) => {
                let full_method = parent_method.into_iter().chain(method).collect::<Vec<_>>();
                match context
                    .call_remote(
                        &full_method.join("."),
                        imbl_value::to_value(&Flat(params, inherited_params))
                            .map_err(parse_error)?,
                    )
                    .await
                {
                    Ok(a) => imbl_value::from_value(a)
                        .map_err(internal_error)
                        .map_err(Self::Err::from),
                    Err(e) => Err(Self::Err::from(e)),
                }
            }
            EitherContext::C2(context) => {
                self.handler
                    .handle_async(HandleArgs {
                        context,
                        parent_method,
                        method,
                        params,
                        inherited_params,
                        raw_params,
                    })
                    .await
            }
        }
    }
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method, ctx_ty)
    }
    fn contexts(&self) -> Option<OrdSet<TypeId>> {
        Self::Context::type_ids()
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method, ctx_ty)
    }
}
impl<Context, H> CliBindings for RemoteCli<Context, H>
where
    H: CliBindings,
{
    type Context = H::Context;
    fn cli_command(&self, ctx_ty: TypeId) -> clap::Command {
        self.handler.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
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
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}

pub struct InheritanceHandler<Params, InheritedParams, H, F> {
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
    fn cli_command(&self, ctx_ty: TypeId) -> clap::Command {
        self.handler.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
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