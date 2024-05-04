use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Debug;

use clap::{CommandFactory, FromArgMatches};
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{Flat, PhantomData};
use crate::{
    CallRemote, CallRemoteHandler, CliBindings, DynHandler, Handler, HandlerArgs, HandlerArgsFor,
    HandlerFor, HandlerTypes, OrEmpty, PrintCliResult, WithContext,
};

pub trait HandlerExt<Context: crate::Context>: HandlerFor<Context> + Sized {
    fn no_cli(self) -> NoCli<Self>;
    fn no_display(self) -> NoDisplay<Self>;
    fn with_custom_display<C: crate::Context, P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            C,
            Params = Self::Params,
            InheritedParams = Self::InheritedParams,
            Ok = Self::Ok,
            Err = Self::Err,
        >;
    fn with_custom_display_fn<C: crate::Context, F>(
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
    fn with_call_remote<C>(self) -> RemoteCaller<C, Context, Self>;
}

impl<Context: crate::Context, T: HandlerFor<Context> + Sized> HandlerExt<Context> for T {
    fn no_cli(self) -> NoCli<Self> {
        NoCli(self)
    }
    fn no_display(self) -> NoDisplay<Self> {
        NoDisplay(self)
    }
    fn with_custom_display<C: crate::Context, P>(self, display: P) -> CustomDisplay<P, Self>
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
    fn with_custom_display_fn<C: crate::Context, F>(
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
    fn with_call_remote<C>(self) -> RemoteCaller<C, Context, Self> {
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
impl<Context, H> HandlerFor<Context> for NoCli<H>
where
    Context: crate::Context,
    H: HandlerFor<Context>,
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
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.0.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.0.method_from_dots(method)
    }
}
impl<Context, H> CliBindings<Context> for NoCli<H>
where
    Context: crate::Context,
    H: HandlerTypes,
{
    const NO_CLI: bool = true;
    fn cli_command(&self) -> clap::Command {
        unimplemented!()
    }
    fn cli_parse(
        &self,
        _: &clap::ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        unimplemented!()
    }
    fn cli_display(&self, _: HandlerArgsFor<Context, Self>, _: Self::Ok) -> Result<(), Self::Err> {
        unimplemented!()
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

impl<Context, H> HandlerFor<Context> for NoDisplay<H>
where
    Context: crate::Context,
    H: HandlerFor<Context>,
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
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.0.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.0.method_from_dots(method)
    }
}
impl<Context, H> PrintCliResult<Context> for NoDisplay<H>
where
    Context: crate::Context,
    H: HandlerTypes,
    H::Params: FromArgMatches + CommandFactory + Serialize,
{
    fn print(&self, _: HandlerArgsFor<Context, Self>, _: Self::Ok) -> Result<(), Self::Err> {
        Ok(())
    }
}
impl<Context, H> CliBindings<Context> for NoDisplay<H>
where
    Context: crate::Context,
    Self: HandlerTypes,
    Self::Params: CommandFactory + FromArgMatches + Serialize,
    Self: PrintCliResult<Context>,
{
    fn cli_command(&self) -> clap::Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
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
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print(handle_args, result)
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

impl<Context, P, H> HandlerFor<Context> for CustomDisplay<P, H>
where
    Context: crate::Context,
    H: HandlerFor<Context>,
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
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method)
    }
}
impl<Context, P, H> PrintCliResult<Context> for CustomDisplay<P, H>
where
    Context: crate::Context,
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
impl<Context, P, H> CliBindings<Context> for CustomDisplay<P, H>
where
    Context: crate::Context,
    Self: HandlerTypes,
    Self::Params: CommandFactory + FromArgMatches + Serialize,
    Self: PrintCliResult<Context>,
{
    fn cli_command(&self) -> clap::Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
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
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print(handle_args, result)
    }
}

pub struct CustomDisplayFn<F, H, Context> {
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

impl<Context, F, H, C> HandlerFor<Context> for CustomDisplayFn<F, H, C>
where
    Context: crate::Context,
    C: 'static,
    H: HandlerFor<Context>,
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
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method)
    }
}
impl<F, H, Context> PrintCliResult<Context> for CustomDisplayFn<F, H, Context>
where
    Context: crate::Context,
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
impl<Context, F, H, C> CliBindings<Context> for CustomDisplayFn<F, H, C>
where
    Context: crate::Context,
    Self: HandlerTypes,
    Self::Params: CommandFactory + FromArgMatches + Serialize,
    Self: PrintCliResult<Context>,
{
    fn cli_command(&self) -> clap::Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
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
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print(handle_args, result)
    }
}

pub struct RemoteCaller<Context, RemoteContext, H> {
    _phantom: PhantomData<(Context, RemoteContext)>,
    handler: H,
}
impl<Context, RemoteContext, H: Clone> Clone for RemoteCaller<Context, RemoteContext, H> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, RemoteContext, H: Debug> Debug for RemoteCaller<Context, RemoteContext, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RemoteCaller").field(&self.handler).finish()
    }
}

impl<Context, H, Inherited, LocalContext, RemoteContext> Handler<Inherited>
    for WithContext<Context, RemoteCaller<LocalContext, RemoteContext, H>>
where
    Context: crate::Context,
    LocalContext: crate::Context + CallRemote<RemoteContext>,
    RemoteContext: crate::Context,
    H: HandlerFor<RemoteContext> + CliBindings<LocalContext>,
    H::Ok: Serialize + DeserializeOwned,
    H::Err: From<RpcError>,
    H::Params: Serialize + DeserializeOwned,
    H::InheritedParams: Serialize + OrEmpty<Inherited>,
    RpcError: From<H::Err>,
    Inherited: Send + Sync + 'static,
{
    type H = H;
    fn handler_for<C: crate::Context>(self) -> Option<DynHandler<C, Inherited>> {
        if TypeId::of::<C>() == TypeId::of::<RemoteContext>() {
            DynHandler::new(self.handler.handler.no_cli())
        } else if TypeId::of::<C>() == TypeId::of::<LocalContext>() {
            DynHandler::new(CallRemoteHandler::<LocalContext, RemoteContext, _>::new(
                self.handler.handler,
            ))
        } else {
            None
        }
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

impl<Context, Params, InheritedParams, H, F> HandlerFor<Context>
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: HandlerFor<Context>,
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
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method)
    }
}

impl<Context, Params, InheritedParams, H, F> CliBindings<Context>
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: CliBindings<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
{
    fn cli_command(&self) -> clap::Command {
        self.handler.cli_command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches)
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
