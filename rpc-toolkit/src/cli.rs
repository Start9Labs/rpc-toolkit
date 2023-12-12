use clap::ArgMatches;
use imbl_value::Value;
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::Serialize;
use url::Url;
use yajrc::{GenericRpcMethod, Id, RpcError, RpcRequest};

use crate::command::{AsyncCommand, DynCommand, LeafCommand, ParentInfo};
use crate::util::{combine, invalid_params, parse_error};
use crate::ParentChain;

pub struct CliApp<Context> {
    pub(crate) command: DynCommand<Context>,
    pub(crate) make_ctx: Box<dyn FnOnce(&ArgMatches) -> Result<Context, RpcError>>,
}

#[async_trait::async_trait]
pub trait CliContext {
    async fn call_remote(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

pub trait CliContextHttp {
    fn client(&self) -> &Client;
    fn url(&self) -> Url;
}
#[async_trait::async_trait]
impl<T: CliContextHttp + Sync> CliContext for T {
    async fn call_remote(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let rpc_req: RpcRequest<GenericRpcMethod<&str, Value, Value>> = RpcRequest {
            id: Some(Id::Number(0.into())),
            method: GenericRpcMethod::new(method),
            params,
        };
        let mut req = self.client().request(Method::POST, self.url());
        let body;
        #[cfg(feature = "cbor")]
        {
            req = req.header("content-type", "application/cbor");
            req = req.header("accept", "application/cbor, application/json");
            body = serde_cbor::to_vec(&rpc_req)?;
        }
        #[cfg(not(feature = "cbor"))]
        {
            req = req.header("content-type", "application/json");
            req = req.header("accept", "application/json");
            body = serde_json::to_vec(&req)?;
        }
        let res = req
            .header("content-length", body.len())
            .body(body)
            .send()
            .await?;
        Ok(
            match res
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
            {
                Some("application/json") => serde_json::from_slice(&*res.bytes().await?)?,
                #[cfg(feature = "cbor")]
                Some("application/cbor") => serde_cbor::from_slice(&*res.bytes().await?)?,
                _ => {
                    return Err(RpcError {
                        data: Some("missing content type".into()),
                        ..yajrc::INTERNAL_ERROR
                    })
                }
            },
        )
    }
}

pub trait RemoteCommand<Context: CliContext>: LeafCommand {
    fn subcommands(chain: ParentChain<Self>) -> Vec<DynCommand<Context>> {
        drop(chain);
        Vec::new()
    }
}
#[async_trait::async_trait]
impl<T, Context> AsyncCommand<Context> for T
where
    T: RemoteCommand<Context> + Send + Serialize,
    T::Parent: Serialize,
    T::Ok: DeserializeOwned,
    T::Err: From<RpcError>,
    Context: CliContext + Send + 'static,
{
    async fn implementation(
        self,
        ctx: Context,
        parent: ParentInfo<Self::Parent>,
    ) -> Result<Self::Ok, Self::Err> {
        let mut method = parent.method;
        method.push(Self::NAME);
        Ok(imbl_value::from_value(
            ctx.call_remote(
                &method.join("."),
                combine(
                    imbl_value::to_value(&self).map_err(invalid_params)?,
                    imbl_value::to_value(&parent.args).map_err(invalid_params)?,
                )?,
            )
            .await?,
        )
        .map_err(parse_error)?)
    }
}
