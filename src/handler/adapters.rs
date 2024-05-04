use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Debug;

use clap::{CommandFactory, FromArgMatches};
use imbl_value::imbl::{OrdMap, OrdSet};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{internal_error, Flat, PhantomData};
use crate::{
    AnyContext, CallRemote, CliBindings, EitherContext, Empty, Handler, HandlerArgs,
    HandlerArgsFor, HandlerTypes, IntoContext, PrintCliResult,
};

pub trait HandlerExt<Context: IntoContext>: Handler<Context> + Sized {
    fn no_cli(self) -> NoCli<Self>;
    fn no_display(self) -> NoDisplay<Self>;
    fn with_custom_display<C: IntoContext, P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            C,
            Params = Self::Params,
            InheritedParams = Self::InheritedParams,
            Ok = Self::Ok,
            Err = Self::Err,
        >;
    fn with_custom_display_fn<C: IntoContext, F>(
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
    fn with_call_remote<C>(self) -> RemoteCaller<C, Self>;
}

impl<Context: IntoContext, T: Handler<Context> + Sized> HandlerExt<Context> for T {
    fn no_cli(self) -> NoCli<Self> {
        NoCli(self)
    }
    fn no_display(self) -> NoDisplay<Self> {
        NoDisplay(self)
    }
    fn with_custom_display<C: IntoContext, P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            C,
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
    fn with_custom_display_fn<C: IntoContext, F>(
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
    fn with_call_remote<C>(self) -> RemoteCaller<C, Self> {
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
// TODO: implement Handler

#[derive(Debug, Clone)]
pub struct NoDisplay<H>(pub H);
impl<H: HandlerTypes> HandlerTypes for NoDisplay<H> {
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}

impl<Context, H> Handler<Context> for NoDisplay<H>
where
    Context: IntoContext,
    H: Handler<Context>,
{
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
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
        }: HandlerArgsFor<Context, Self>,
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
impl<Context, H> PrintCliResult<Context> for NoDisplay<H>
where
    Context: IntoContext,
    H: HandlerTypes,
    H::Params: FromArgMatches + CommandFactory + Serialize,
{
    fn print(&self, _: HandlerArgsFor<Context, Self>, _: Self::Ok) -> Result<(), Self::Err> {
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

impl<Context, P, H> Handler<Context> for CustomDisplay<P, H>
where
    Context: IntoContext,
    H: Handler<Context>,
    P: Send + Sync + Clone + 'static,
{
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
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
        }: HandlerArgsFor<Context, Self>,
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
impl<Context, P, H> PrintCliResult<Context> for CustomDisplay<P, H>
where
    Context: IntoContext,
    H: HandlerTypes,
    P: PrintCliResult<
            Context,
            Params = H::Params,
            InheritedParams = H::InheritedParams,
            Ok = H::Ok,
            Err = H::Err,
        > + Send
        + Sync
        + Clone
        + 'static,
{
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

impl<Context, F, H, C> Handler<Context> for CustomDisplayFn<F, H, C>
where
    Context: IntoContext,
    C: 'static,
    H: Handler<Context>,
    F: Send + Sync + Clone + 'static,
{
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
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
        }: HandlerArgsFor<Context, Self>,
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
impl<F, H, Context> PrintCliResult<Context> for CustomDisplayFn<F, H, Context>
where
    Context: IntoContext,
    H: HandlerTypes,
    F: Fn(HandlerArgsFor<Context, H>, H::Ok) -> Result<(), H::Err> + Send + Sync + Clone + 'static,
{
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

pub struct RemoteCaller<Context, H> {
    _phantom: PhantomData<Context>,
    handler: H,
}
impl<Context, H: Clone> Clone for RemoteCaller<Context, H> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, H: Debug> Debug for RemoteCaller<Context, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RemoteCaller").field(&self.handler).finish()
    }
}
impl<Context, H> HandlerTypes for RemoteCaller<Context, H>
where
    H: HandlerTypes,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}

impl<Context, RemoteContext, H> Handler<EitherContext<Context, RemoteContext>>
    for RemoteCaller<Context, H>
where
    Context: CallRemote<RemoteContext>,
    RemoteContext: IntoContext,
    H: Handler<RemoteContext>,
    H::Params: Serialize,
    H::InheritedParams: Serialize,
    H::Ok: DeserializeOwned,
    H::Err: From<RpcError>,
{
    async fn handle_async(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<EitherContext<Context, RemoteContext>, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        match context {
            EitherContext::C1(context) => {
                let full_method = parent_method.into_iter().chain(method).collect::<Vec<_>>();
                match context
                    .call_remote(&full_method.join("."), raw_params, Empty {})
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
        Context::type_ids()
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method, ctx_ty)
    }
}
impl<Context, H> PrintCliResult<Context> for RemoteCaller<Context, H>
where
    Context: IntoContext,
    H: PrintCliResult<Context>,
{
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

impl<Context, Params, InheritedParams, H, F> Handler<Context>
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Context: IntoContext,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: Handler<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
{
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
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
        }: HandlerArgsFor<Context, Self>,
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

impl<Context, Params, InheritedParams, H, F> CliBindings<Context>
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Context: IntoContext,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: CliBindings<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
{
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
        }: HandlerArgsFor<Context, Self>,
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
