use std::sync::Arc;

use core::fmt::Debug;
use html_languageservice::{HTMLDataManager, HTMLLanguageService, HTMLLanguageServiceOptions};
use lsp_textdocument::TextDocuments;
use serde_json::Value;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionResponse, CompletionItem, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, ExecuteCommandParams,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, InitializeParams,
    InitializeResult, InitializedParams, Position, RenameFilesParams, SemanticTokensParams,
    SemanticTokensRangeParams, SemanticTokensRangeResult, SemanticTokensResult, ServerCapabilities,
    ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};
use tracing::{debug, info, instrument};

use crate::renderer::{PositionType, Renderer};
use crate::ts_server::TsServer;

pub struct VueLspServer {
    _client: Client,
    is_shared: bool,
    text_documents: Arc<Mutex<TextDocuments>>,
    data_manager: Mutex<HTMLDataManager>,
    html_server: Mutex<HTMLLanguageService>,
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
        let renderer = Arc::new(Mutex::new(Renderer::new()));
        let ts_server = Arc::new(Mutex::new(TsServer::new(
            client.clone(),
            Arc::clone(&renderer),
        )));
        let data_manager = Mutex::new(HTMLDataManager::default());
        let html_server = Mutex::new(HTMLLanguageService::new(
            &HTMLLanguageServiceOptions::default(),
        ));
        VueLspServer {
            _client: client,
            is_shared,
            text_documents,
            data_manager,
            html_server,
            ts_server,
            renderer,
        }
    }

    /// 在进行 html 服务器相关的操作前调用
    async fn update_html_languageservice(&self, uri: &Url) {
        debug!("(Vue2TsDecoratorServer/update_html_languageservice)");
        let mut renderer = self.renderer.lock().await;
        let tags_provider = renderer.get_tags_provider(uri).await;

        let mut data_manager = self.data_manager.lock().await;
        data_manager.set_data_providers(true, vec![Box::new(tags_provider.clone())]);

        let mut html_server = self.html_server.lock().await;
        html_server.set_completion_participants(vec![Box::new(tags_provider)]);
        debug!("(Vue2TsDecoratorServer/update_html_languageservice) done");
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
            let result = self.ts_server.lock().await.initialize(params).await?;
            Ok(InitializeResult {
                server_info: Some(ServerInfo {
                    name: "vue-property-decorator-extension-server".to_string(),
                    version: Some("1.0.0".to_string()),
                }),
                capabilities: ServerCapabilities {
                    text_document_sync: Some(TextDocumentSyncCapability::Kind(
                        TextDocumentSyncKind::INCREMENTAL,
                    )),
                    hover_provider: result.capabilities.hover_provider,
                    completion_provider: result.capabilities.completion_provider,
                    definition_provider: result.capabilities.definition_provider,
                    document_symbol_provider: result.capabilities.document_symbol_provider,
                    semantic_tokens_provider: result.capabilities.semantic_tokens_provider,
                    code_action_provider: result.capabilities.code_action_provider,
                    ..Default::default()
                },
            })
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
        let uri = &params.text_document.uri;
        let text_documents = self.text_documents.lock().await;
        let document = text_documents.get_document(uri).unwrap();
        self.ts_server.lock().await.did_open(uri, document).await;

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
        let uri = &params.text_document.uri.clone();
        let text_documents = self.text_documents.lock().await;
        let document = text_documents.get_document(uri).unwrap();
        self.ts_server
            .lock()
            .await
            .did_change(params, document)
            .await;
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
        self.ts_server.lock().await.did_close(params).await;
        info!("done");
    }

    #[instrument]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        info!("start");
        self.renderer.lock().await.save(&params.text_document.uri);
        info!("done");
    }

    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        info!("will_rename_files start");
        let response = self.ts_server.lock().await.will_rename_files(params).await;
        info!("will_rename_files done");
        response
    }

    #[instrument]
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        info!("start");
        let mut hover = Ok(None);
        let uri = &params.text_document_position_params.text_document.uri;
        let position = &params.text_document_position_params.position;
        let typ = {
            let renderer = self.renderer.lock().await;
            renderer.get_position_type(uri, position)
        };
        if let Some(typ) = typ {
            match typ {
                PositionType::Script => {
                    info!("In script");
                    hover = self
                        .ts_server
                        .lock()
                        .await
                        .hover(params.text_document_position_params)
                        .await;
                }
                PositionType::TemplateExpr(pos) => {
                    info!("In template expr");
                    let mut params = params.clone();
                    params.text_document_position_params.position = pos;
                    hover = self
                        .ts_server
                        .lock()
                        .await
                        .hover(params.text_document_position_params)
                        .await;
                }
                PositionType::Template => {
                    info!("In template");
                    self.update_html_languageservice(uri).await;
                    let renderer = self.renderer.lock().await;
                    if let Some(html_document) = renderer.get_html_document(uri) {
                        let html_server = self.html_server.lock().await;
                        let data_manager = self.data_manager.lock().await;
                        let text_documents = self.text_documents.lock().await;
                        if let Some(text_document) = text_documents.get_document(uri) {
                            hover = Ok(html_server
                                .do_hover(
                                    text_document,
                                    position,
                                    &html_document,
                                    None,
                                    &data_manager,
                                )
                                .await);
                        }
                    }
                }
            }
        }
        info!("done");
        hover
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.ts_server.lock().await.completion(params).await
    }

    async fn completion_resolve(&self, mut params: CompletionItem) -> Result<CompletionItem> {
        /// 判断是否来自 ts_server 并且移除标记，返回原始 uri
        fn get_original_uri(params: &mut CompletionItem) -> Option<Value> {
            let data = params.data.as_mut()?;
            if data.is_object() {
                let data = data.as_object_mut()?;
                if data.contains_key("from_ts_server") {
                    data.remove("from_ts_server");
                    Some(data.remove("original_uri").unwrap())
                } else {
                    None
                }
            } else {
                None
            }
        }
        let original_uri = get_original_uri(&mut params);
        if let Some(original_uri) = original_uri {
            self.ts_server
                .lock()
                .await
                .completion_resolve(params, serde_json::from_value(original_uri).unwrap())
                .await
        } else {
            Ok(params)
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        // TODO: is_in_template
        self.ts_server
            .lock()
            .await
            .goto_definition(params, false)
            .await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        // TODO: script 的 document_symbol 添加到 html 下
        self.ts_server.lock().await.document_symbol(params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.ts_server
            .lock()
            .await
            .semantic_tokens_full(params)
            .await
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        self.ts_server
            .lock()
            .await
            .semantic_tokens_range(params)
            .await
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        self.ts_server.lock().await.code_action(params).await
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        let text_documents = self.text_documents.lock().await;
        if params.command == "vue2-ts-decorator.restart.tsserver" {
            self.ts_server.lock().await.restart(&text_documents).await;
            Ok(None)
        } else {
            self.ts_server.lock().await.execute_command(params).await
        }
    }

    async fn shutdown(&self) -> Result<()> {
        self.ts_server.lock().await.shutdown().await;
        Ok(())
    }
}
