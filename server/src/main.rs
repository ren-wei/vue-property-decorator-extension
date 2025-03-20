use tower_lsp::{LspService, Server};
use vue_property_decorator_extension_server::{log::LspSubscriber, server::VueLspServer};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| {
        let subscriber = LspSubscriber::new(client.clone());
        tracing::subscriber::set_global_default(subscriber).unwrap();
        VueLspServer::new(client, None)
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
