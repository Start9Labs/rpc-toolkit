use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::sync::Arc;

use clap::{CommandFactory, FromArgMatches};
use imbl_value::imbl::{OrdMap, OrdSet};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{internal_error, invalid_params, without, Flat, PhantomData};
use crate::{
    iter_from_ctx_and_handler, AnyContext, AnyHandler, CallRemote, CliBindings, DynHandler,
    EitherContext, Empty, Handler, HandlerArgs, HandlerArgsFor, HandlerTypes, IntoContext,
    IntoHandlers, OrEmpty, PrintCliResult,
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
    ) -> CustomDisplayFn<F, Self, Context>
    where
        F: Fn(HandlerArgsFor<Context, Self>, Self::Ok) -> Result<(), Self::Err>;
    fn with_inherited<Params, InheritedParams, F>(
        self,
        f: F,
    ) -> InheritanceHandler<Params, InheritedParams, Self, F>
    where
        F: Fn(Params, InheritedParams) -> Self::InheritedParams;
    fn with_call_remote<Context, Extra>(self) -> RemoteCaller<Context, Self, Extra>;
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
    ) -> CustomDisplayFn<F, Self, Context>
    where
        F: Fn(HandlerArgsFor<Context, Self>, Self::Ok) -> Result<(), Self::Err>,
    {
        CustomDisplayFn {
            _phantom: PhantomData::new(),
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
            _phantom: PhantomData::new(),
            handler: self,
            inherit: f,
        }
    }
    fn with_call_remote<Context, Extra>(self) -> RemoteCaller<Context, Self, Extra> {
        RemoteCaller {
            _phantom: PhantomData::new(),
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
impl<H, A, B> IntoHandlers<Flat<A, B>> for NoCli<H>
where
    H: Handler,
    H::Params: DeserializeOwned,
    H::InheritedParams: OrEmpty<Flat<A, B>>,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
    A: Send + Sync + 'static,
    B: Send + Sync + 'static,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, DynHandler<Flat<A, B>>)> {
        iter_from_ctx_and_handler(
            self.0.contexts(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(
                self.0.with_inherited(|a, b| OrEmpty::from_t(Flat(a, b))),
            ))),
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

impl<H> Handler for NoDisplay<H>
where
    H: Handler,
{
    type Context = H::Context;
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.0.handle_sync(HandlerArgs {
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
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.0
            .handle_async(HandlerArgs {
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
impl<H> PrintCliResult for NoDisplay<H>
where
    H: HandlerTypes,
    H::Params: FromArgMatches + CommandFactory + Serialize,
{
    type Context = AnyContext;
    fn print(&self, _: HandlerArgsFor<Self::Context, Self>, _: Self::Ok) -> Result<(), Self::Err> {
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

impl<P, H> Handler for CustomDisplay<P, H>
where
    H: Handler,
    P: Send + Sync + Clone + 'static,
{
    type Context = H::Context;
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandlerArgs {
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
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler
            .handle_async(HandlerArgs {
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
impl<P, H> PrintCliResult for CustomDisplay<P, H>
where
    H: HandlerTypes,
    P: PrintCliResult<
            Params = H::Params,
            InheritedParams = H::InheritedParams,
            Ok = H::Ok,
            Err = H::Err,
        > + Send
        + Sync
        + Clone
        + 'static,
{
    type Context = P::Context;
    fn print(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print.print(
            HandlerArgs {
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

pub struct CustomDisplayFn<F, H, Context = AnyContext> {
    _phantom: PhantomData<Context>,
    print: F,
    handler: H,
}
impl<Context, F: Clone, H: Clone> Clone for CustomDisplayFn<F, H, Context> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            print: self.print.clone(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, F: Debug, H: Debug> Debug for CustomDisplayFn<F, H, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomDisplayFn")
            .field("print", &self.print)
            .field("handler", &self.handler)
            .finish()
    }
}
impl<F, H, Context> HandlerTypes for CustomDisplayFn<F, H, Context>
where
    H: HandlerTypes,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}

impl<F, H, Context> Handler for CustomDisplayFn<F, H, Context>
where
    Context: Send + Sync + 'static,
    H: Handler,
    F: Send + Sync + Clone + 'static,
{
    type Context = H::Context;
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandlerArgs {
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
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler
            .handle_async(HandlerArgs {
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
impl<F, H, Context> PrintCliResult for CustomDisplayFn<F, H, Context>
where
    Context: IntoContext,
    H: HandlerTypes,
    F: Fn(HandlerArgsFor<Context, H>, H::Ok) -> Result<(), H::Err> + Send + Sync + Clone + 'static,
{
    type Context = Context;
    fn print(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        (self.print)(
            HandlerArgs {
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

pub struct RemoteCaller<Context, H, Extra = Empty> {
    _phantom: PhantomData<(Context, Extra)>,
    handler: H,
}
impl<Context, H: Clone, Extra> Clone for RemoteCaller<Context, H, Extra> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, H: Debug, Extra> Debug for RemoteCaller<Context, H, Extra> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RemoteCaller").field(&self.handler).finish()
    }
}
impl<Context, H, Extra> HandlerTypes for RemoteCaller<Context, H, Extra>
where
    H: HandlerTypes,
    Extra: Send + Sync + 'static,
{
    type Params = Flat<H::Params, Extra>;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}

impl<Context, H, Extra> Handler for RemoteCaller<Context, H, Extra>
where
    Context: CallRemote<H::Context, Extra>,
    H: Handler,
    H::Params: Serialize,
    H::InheritedParams: Serialize,
    H::Ok: DeserializeOwned,
    H::Err: From<RpcError>,
    Extra: Serialize + Send + Sync + 'static,
{
    type Context = EitherContext<Context, H::Context>;
    async fn handle_async(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params: Flat(params, extra),
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        match context {
            EitherContext::C1(context) => {
                let full_method = parent_method.into_iter().chain(method).collect::<Vec<_>>();
                match context
                    .call_remote(
                        &full_method.join("."),
                        without(raw_params, &extra).map_err(invalid_params)?,
                        extra,
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
                    .handle_async(HandlerArgs {
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
impl<Context, H> PrintCliResult for RemoteCaller<Context, H>
where
    Context: IntoContext,
    H: PrintCliResult,
{
    type Context = H::Context;
    fn print(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params: Flat(params, _),
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.print(
            HandlerArgs {
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
            _phantom: PhantomData::new(),
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
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandlerArgs {
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
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler
            .handle_async(HandlerArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
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
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.cli_display(
            HandlerArgs {
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
