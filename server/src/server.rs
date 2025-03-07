use std::sync::Arc;

use core::fmt::Debug;
use lsp_textdocument::TextDocuments;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Hover,
    HoverParams, InitializeParams, InitializeResult, InitializedParams, ServerInfo,
};
use tower_lsp::{Client, LanguageServer};
use tracing::{error, info, instrument, warn};

use crate::renderer::Renderer;
use crate::ts_server::TsServer;

pub struct VueLspServer {
    _client: Client,
    is_shared: bool,
    text_documents: Arc<Mutex<TextDocuments>>,
    ts_server: Arc<Mutex<TsServer>>,
    renderer: Arc<Mutex<Renderer>>,
}

impl VueLspServer {
    pub fn new(
        client: Client,
        shared_text_documents: Option<Arc<Mutex<TextDocuments>>>,
    ) -> VueLspServer {
        let is_shared;
        let text_documents = if let Some(shared_text_documents) = shared_text_documents {
            is_shared = true;
            shared_text_documents
        } else {
            is_shared = false;
            Arc::new(Mutex::new(TextDocuments::new()))
        };
        let ts_server = Arc::new(Mutex::new(TsServer::new(client.clone())));
        let renderer = Arc::new(Mutex::new(Renderer::new()));
        VueLspServer {
            _client: client,
            is_shared,
            text_documents,
            ts_server,
            renderer,
        }
    }
}

impl Debug for VueLspServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LspServer")
            .field("_client", &self._client)
            .field("is_shared", &self.is_shared)
            .finish()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for VueLspServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(root_uri) = &params.root_uri {
            self.renderer.lock().await.init(root_uri).await;
            self.ts_server.lock().await.initialize(params).await
        } else {
            Ok(InitializeResult {
                server_info: Some(ServerInfo {
                    name: "vue-property-decorator-extension-server".to_string(),
                    version: Some("1.0.0".to_string()),
                }),
                ..Default::default()
            })
        }
    }

    #[instrument]
    async fn initialized(&self, _params: InitializedParams) {
        info!("start");
        self.ts_server.lock().await.initialized().await;
        info!("done");
    }

    #[instrument]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        info!("start");
        if !self.is_shared {
            let mut text_documents = self.text_documents.lock().await;
            text_documents.listen(
                "textDocument/didOpen",
                &serde_json::to_value(&params).unwrap(),
            );
        }
        info!("done");
    }

    #[instrument]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        info!("start");
        if !self.is_shared {
            let mut text_documents = self.text_documents.lock().await;
            text_documents.listen(
                "textDocument/didChange",
                &serde_json::to_value(&params).unwrap(),
            );
        }
        info!("done");
    }

    #[instrument]
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        info!("start");
        if !self.is_shared {
            let mut text_documents = self.text_documents.lock().await;
            text_documents.listen(
                "textDocument/didClose",
                &serde_json::to_value(&params).unwrap(),
            );
        }
        info!("done");
    }

    async fn hover(&self, _params: HoverParams) -> Result<Option<Hover>> {
        error!("method not found");
        Err(Error::method_not_found())
    }

    async fn shutdown(&self) -> Result<()> {
        warn!("shutdown");
        Ok(())
    }
}
