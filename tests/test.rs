// use std::path::PathBuf;

// use clap::Parser;
// use futures::Future;
// use rpc_toolkit::{
//     AsyncCommand, CliContextSocket, Command, Contains, Context, DynCommand, LeafCommand, NoParent,
//     ParentCommand, ParentInfo, Server, ShutdownHandle,
// };
// use serde::{Deserialize, Serialize};
// use tokio::net::UnixStream;
// use yajrc::RpcError;

// struct ServerContext;
// impl Context for ServerContext {
//     type Metadata = ();
// }

// struct CliContext(PathBuf);
// impl Context for CliContext {
//     type Metadata = ();
// }

// impl CliContextSocket for CliContext {
//     type Stream = UnixStream;
//     async fn connect(&self) -> std::io::Result<Self::Stream> {
//         UnixStream::connect(&self.0).await
//     }
// }

// impl rpc_toolkit::CliContext for CliContext {
//     async fn call_remote(
//         &self,
//         method: &str,
//         params: imbl_value::Value,
//     ) -> Result<imbl_value::Value, RpcError> {
//         <Self as CliContextSocket>::call_remote(self, method, params).await
//     }
// }

// async fn run_server() {
//     Server::new(
//         vec![
//             DynCommand::from_parent::<Group>(Contains::none()),
//             DynCommand::from_async::<Thing1>(Contains::none()),
//             // DynCommand::from_async::<Thing2>(Contains::none()),
//             // DynCommand::from_sync::<Thing3>(Contains::none()),
//             // DynCommand::from_sync::<Thing4>(Contains::none()),
//         ],
//         || async { Ok(ServerContext) },
//     )
//     .run_unix("./test.sock", |e| eprintln!("{e}"))
//     .unwrap()
//     .1
//     .await
// }

// #[derive(Debug, Deserialize, Serialize, Parser)]
// struct Group {
//     #[arg(short, long)]
//     verbose: bool,
// }
// impl Command for Group {
//     const NAME: &'static str = "group";
//     type Parent = NoParent;
// }
// impl<Ctx> ParentCommand<Ctx> for Group
// where
//     Ctx: Context,
//     // SubThing: AsyncCommand<Ctx>,
//     Thing1: AsyncCommand<Ctx>,
// {
//     fn subcommands(chain: rpc_toolkit::ParentChain<Self>) -> Vec<DynCommand<Ctx>> {
//         vec![
//             // DynCommand::from_async::<SubThing>(chain.child()),
//             DynCommand::from_async::<Thing1>(Contains::none()),
//         ]
//     }
// }

// #[derive(Debug, Deserialize, Serialize, Parser)]
// struct Thing1 {
//     thing: String,
// }
// impl Command for Thing1 {
//     const NAME: &'static str = "thing1";
//     type Parent = NoParent;
// }
// impl LeafCommand<ServerContext> for Thing1 {
//     type Ok = String;
//     type Err = RpcError;
//     fn display(self, _: ServerContext, _: rpc_toolkit::ParentInfo<Self::Parent>, res: Self::Ok) {
//         println!("{}", res);
//     }
// }

// impl AsyncCommand<ServerContext> for Thing1 {
//     async fn implementation(
//         self,
//         _: ServerContext,
//         _: ParentInfo<Self::Parent>,
//     ) -> Result<Self::Ok, Self::Err> {
//         Ok(format!("Thing1 is {}", self.thing))
//     }
// }

// #[tokio::test]
// async fn test() {
//     let server = tokio::spawn(run_server());
// }
