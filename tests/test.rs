#![recursion_limit = "512"]

use clap::Parser;
use rpc_toolkit::ts::HandlerTSBindings;
use rpc_toolkit::{
    from_fn, from_fn_async, impl_ts_struct, Context, Empty, HandlerExt, ParentHandler, Server,
};
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

#[derive(Clone)]
struct TestContext;

impl Context for TestContext {}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[cfg_attr(feature = "ts", derive(visit_rs::VisitFields))]
struct Thing1Params {
    thing: String,
}
#[cfg(feature = "ts")]
impl_ts_struct!(Thing1Params);

#[derive(Debug, Deserialize, Serialize, Parser)]
struct NoTSParams {
    foo: String,
}

async fn thing1_handler(_ctx: TestContext, params: Thing1Params) -> Result<String, RpcError> {
    Ok(format!("Thing1 is {}", params.thing))
}

fn no_ts_handler(_ctx: TestContext, params: NoTSParams) -> Result<String, RpcError> {
    Ok(format!("foo:{}", params.foo))
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[cfg_attr(feature = "ts", derive(visit_rs::VisitFields))]
struct GroupParams {
    #[arg(short, long)]
    verbose: bool,
}
#[cfg(feature = "ts")]
impl_ts_struct!(GroupParams);

#[tokio::test]
async fn test_basic_server() {
    let root_handler = ParentHandler::new()
        .subcommand("thing1", from_fn_async(thing1_handler))
        .subcommand(
            "group",
            ParentHandler::<TestContext, Empty, Empty>::new()
                .subcommand("thing1", from_fn_async(thing1_handler))
                .subcommand(
                    "thing2",
                    from_fn_async(|_ctx: TestContext, params: GroupParams| async move {
                        Ok::<_, RpcError>(format!("verbose: {}", params.verbose))
                    }),
                )
                .subcommand("no-ts", from_fn(no_ts_handler).no_ts()),
        );

    println!(
        "{:?}",
        root_handler.get_ts().map(|t| {
            use rpc_toolkit::ts::TSVisitor;
            use visit_rs::Visit;

            let mut ts = TSVisitor::new();
            t.visit(&mut ts);
            ts
        })
    );

    let server = Server::new(|| async { Ok(TestContext) }, root_handler);

    // Test calling thing1 directly
    let result = server
        .handle_command(
            "thing1",
            imbl_value::to_value(&Thing1Params {
                thing: "test".to_string(),
            })
            .unwrap(),
        )
        .await
        .unwrap();

    let response: String = imbl_value::from_value(result).unwrap();
    assert_eq!(response, "Thing1 is test");

    // Test calling group.thing1
    let result = server
        .handle_command(
            "group.thing1",
            imbl_value::to_value(&Thing1Params {
                thing: "nested".to_string(),
            })
            .unwrap(),
        )
        .await
        .unwrap();

    let response: String = imbl_value::from_value(result).unwrap();
    assert_eq!(response, "Thing1 is nested");
}
