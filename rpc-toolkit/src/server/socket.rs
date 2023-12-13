use futures::{AsyncWrite, Future, Stream};
use tokio::io::AsyncRead;
use tokio::sync::oneshot;
use yajrc::RpcError;

use crate::Server;

pub struct ShutdownHandle(oneshot::Sender<()>);

pub struct SocketServer<Context: crate::Context> {
    server: Server<Context>,
}
impl<Context: crate::Context> SocketServer<Context> {
    pub fn run_json<T: AsyncRead + AsyncWrite>(
        &self,
        listener: impl Stream<Item = T>,
    ) -> (ShutdownHandle, impl Future<Output = Result<(), RpcError>>) {
        let (shutdown_send, shutdown_recv) = oneshot::channel();
        (ShutdownHandle(shutdown_send), async move {
            //asdf
            //adf
            Ok(())
        })
    }
}
