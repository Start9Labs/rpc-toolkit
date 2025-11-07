use clap::Parser;
use rpc_toolkit::{from_fn_async, Context, Empty, HandlerTS, ParentHandler, Server};
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

#[derive(Clone)]
struct TestContext;

impl Context for TestContext {}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
struct Thing1Params {
    thing: String,
}

async fn thing1_handler(_ctx: TestContext, params: Thing1Params) -> Result<String, RpcError> {
    Ok(format!("Thing1 is {}", params.thing))
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
struct GroupParams {
    #[arg(short, long)]
    verbose: bool,
}

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
                ),
        );

    println!("{}", root_handler.type_info().unwrap_or_default());

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
