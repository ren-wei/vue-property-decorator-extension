use std::sync::Arc;

use core::fmt::Debug;
use html_languageservice::parser::html_document::Node;
use html_languageservice::parser::html_scanner::TokenType;
use html_languageservice::{
    DefaultDocumentContext, HTMLDataManager, HTMLLanguageService, HTMLLanguageServiceOptions,
};
use lsp_textdocument::TextDocuments;
use serde_json::{json, Value};
use std::time;
use tokio::sync::{Mutex, RwLock};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification,
};
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionResponse, CompletionItem, CompletionParams, CompletionResponse,
    CreateFilesParams, DeleteFilesParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams, FileOperationFilter,
    FileOperationPattern, FileOperationRegistrationOptions, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, InitializeParams, InitializeResult,
    InitializedParams, RenameFilesParams, SemanticTokensParams, SemanticTokensRangeParams,
    SemanticTokensRangeResult, SemanticTokensResult, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkDoneProgressOptions, WorkspaceEdit,
    WorkspaceFileOperationsServerCapabilities, WorkspaceServerCapabilities,
};
use tower_lsp::{Client, LanguageServer};
use tracing::{debug, info, instrument};

use crate::renderer::{Mapping, PositionType, Renderer};
use crate::ts_server::TsServer;

pub struct VueLspServer {
    is_shared: bool,
    text_documents: Arc<Mutex<TextDocuments>>,
    data_manager: Mutex<HTMLDataManager>,
    html_server: Mutex<HTMLLanguageService>,
    ts_server: Arc<RwLock<TsServer>>,
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
        let ts_server = Arc::new(RwLock::new(TsServer::new(client, Arc::clone(&renderer))));
        let data_manager = Mutex::new(HTMLDataManager::default());
        let html_server = Mutex::new(HTMLLanguageService::new(
            &HTMLLanguageServiceOptions::default(),
        ));
        VueLspServer {
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
        let tags_provider = {
            let mut renderer = self.renderer.lock().await;
            renderer.get_tags_provider(uri).await
        };

        let mut data_manager = self.data_manager.lock().await;
        data_manager.set_data_providers(true, vec![Box::new(tags_provider.clone())]);

        let mut html_server = self.html_server.lock().await;
        html_server.set_completion_participants(vec![Box::new(tags_provider)]);
        debug!("(Vue2TsDecoratorServer/update_html_languageservice) done");
    }

    /// 是否处理 uri
    fn is_uri_valid(uri: &Url) -> bool {
        !uri.path().contains("/node_modules/")
    }
}

impl Debug for VueLspServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LspServer")
            .field("is_shared", &self.is_shared)
            .finish()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for VueLspServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(root_uri) = &params.root_uri {
            self.renderer.lock().await.init(root_uri).await;
            let result = self.ts_server.write().await.initialize(params).await?;
            let file_operation = Some(FileOperationRegistrationOptions {
                filters: vec![FileOperationFilter {
                    scheme: Some("file".to_string()),
                    pattern: FileOperationPattern {
                        glob: "**".to_string(),
                        ..Default::default()
                    },
                }],
            });
            let mut commands = vec!["vue2-ts-decorator.restart.tsserver".to_string()];
            if let Some(execute_command_provider) = result.capabilities.execute_command_provider {
                let mut ts_commands = execute_command_provider
                    .commands
                    .iter()
                    .map(|c| c.replace("_typescript", "vue2-ts-decorator_typescript"))
                    .collect::<Vec<String>>();
                commands.append(&mut ts_commands);
            }
            Ok(InitializeResult {
                server_info: Some(ServerInfo {
                    name: "vue-property-decorator-extension-server".to_string(),
                    version: Some("1.0.0".to_string()),
                }),
                capabilities: ServerCapabilities {
                    position_encoding: result.capabilities.position_encoding,
                    text_document_sync: Some(TextDocumentSyncCapability::Kind(
                        TextDocumentSyncKind::INCREMENTAL,
                    )),
                    hover_provider: result.capabilities.hover_provider,
                    completion_provider: result.capabilities.completion_provider,
                    definition_provider: result.capabilities.definition_provider,
                    document_symbol_provider: result.capabilities.document_symbol_provider,
                    semantic_tokens_provider: result.capabilities.semantic_tokens_provider,
                    code_action_provider: result.capabilities.code_action_provider,
                    workspace: Some(WorkspaceServerCapabilities {
                        workspace_folders: result
                            .capabilities
                            .workspace
                            .map(|w| w.workspace_folders)
                            .flatten(),
                        file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                            will_create: file_operation.clone(),
                            did_create: file_operation.clone(),
                            will_rename: file_operation.clone(),
                            did_rename: file_operation.clone(),
                            will_delete: None,
                            did_delete: file_operation,
                        }),
                    }),
                    execute_command_provider: Some(ExecuteCommandOptions {
                        commands,
                        work_done_progress_options: WorkDoneProgressOptions {
                            work_done_progress: None,
                        },
                    }),
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
        self.ts_server.read().await.initialized().await;
        info!("done");
    }

    #[instrument]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        if !VueLspServer::is_uri_valid(&params.text_document.uri) {
            return;
        }
        info!("start");
        let start_time = time::Instant::now();
        if !self.is_shared {
            let mut text_documents = self.text_documents.lock().await;
            text_documents.listen(
                DidOpenTextDocument::METHOD,
                &serde_json::to_value(&params).unwrap(),
            );
        }
        let uri = params.text_document.uri.clone();
        let renderer = Arc::clone(&self.renderer);
        let text_documents = Arc::clone(&self.text_documents);
        let ts_server = Arc::clone(&self.ts_server);
        {
            renderer.lock().await.did_open(&uri).await;
        }
        tokio::spawn(async move {
            loop {
                let is_wait = {
                    let renderer = renderer.lock().await;
                    renderer.is_wait_create(&uri)
                };
                if is_wait {
                    tokio::task::yield_now().await;
                } else {
                    break;
                }
            }
            debug!("did_open:lock text_documents await");
            let text_documents = text_documents.lock().await;
            debug!("did_open:lock text_documents");
            let document = text_documents.get_document(&uri).unwrap();
            debug!("did_open:lock ts_server await");
            let mut ts_server = ts_server.write().await;
            debug!("did_open:lock ts_server");
            ts_server.did_open(&uri, document).await;
            info!("did_open:done {:?}", start_time.elapsed());
        });
    }

    #[instrument]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if !VueLspServer::is_uri_valid(&params.text_document.uri) {
            return;
        }
        info!("start");
        let start_time = time::Instant::now();
        if !self.is_shared {
            let mut text_documents = self.text_documents.lock().await;
            text_documents.listen(
                DidChangeTextDocument::METHOD,
                &serde_json::to_value(&params).unwrap(),
            );
        }
        let uri = &params.text_document.uri.clone();
        let text_documents = self.text_documents.lock().await;
        let document = text_documents.get_document(uri).unwrap();
        debug!("lock ts_server await");
        let mut ts_server = self.ts_server.write().await;
        debug!("lock ts_server");
        ts_server.did_change(params, document).await;
        info!("done {:?}", start_time.elapsed());
    }

    #[instrument]
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if !VueLspServer::is_uri_valid(&params.text_document.uri) {
            return;
        }
        info!("start");
        let start_time = time::Instant::now();
        if !self.is_shared {
            let mut text_documents = self.text_documents.lock().await;
            text_documents.listen(
                DidCloseTextDocument::METHOD,
                &serde_json::to_value(&params).unwrap(),
            );
        }
        self.ts_server.read().await.did_close(params).await;
        info!("done {:?}", start_time.elapsed());
    }

    #[instrument]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = &params.text_document.uri;
        if !VueLspServer::is_uri_valid(uri) {
            return;
        }
        info!("start");
        let start_time = time::Instant::now();
        let change = {
            self.renderer
                .lock()
                .await
                .save(&params.text_document.uri)
                .await
        };
        if let Some(change) = change {
            debug!("lock ts_server await");
            let mut ts_server = self.ts_server.write().await;
            debug!("lock ts_server");
            ts_server.did_save(change).await;
        }
        info!("done {:?}", start_time.elapsed());
    }

    async fn will_create_files(&self, params: CreateFilesParams) -> Result<Option<WorkspaceEdit>> {
        self.renderer.lock().await.will_create_files(&params);
        Ok(None)
    }

    #[instrument]
    async fn did_create_files(&self, params: CreateFilesParams) {
        debug!("start");
        let start_time = time::Instant::now();
        self.renderer.lock().await.did_create_files(params).await;
        debug!("done: {:?}", start_time.elapsed());
    }

    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        {
            self.renderer.lock().await.will_rename_files(&params);
        }
        let response = self.ts_server.read().await.will_rename_files(params).await;
        response
    }

    async fn did_rename_files(&self, params: RenameFilesParams) {
        self.renderer.lock().await.did_rename_files(params).await;
    }

    #[instrument]
    async fn did_delete_files(&self, params: DeleteFilesParams) {
        debug!("start");
        let start_time = time::Instant::now();
        self.renderer.lock().await.did_delete_files(params).await;
        debug!("done: {:?}", start_time.elapsed());
    }

    #[instrument]
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        if !VueLspServer::is_uri_valid(&params.text_document_position_params.text_document.uri) {
            return Ok(None);
        }
        info!("start");
        let start_time = time::Instant::now();
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
                        .read()
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
                        .read()
                        .await
                        .hover(params.text_document_position_params)
                        .await;
                }
                PositionType::Template => {
                    info!("In template");
                    self.update_html_languageservice(uri).await;
                    let html_document = {
                        let renderer = self.renderer.lock().await;
                        renderer.get_html_document(uri)
                    };
                    if let Some(html_document) = html_document {
                        let data_manager = self.data_manager.lock().await;
                        let html_server = self.html_server.lock().await;
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
        info!("done {:?}", start_time.elapsed());
        hover
    }

    #[instrument]
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        if !VueLspServer::is_uri_valid(&params.text_document_position.text_document.uri) {
            return Ok(None);
        }
        info!("start");
        let start_time = time::Instant::now();
        let uri = &params.text_document_position.text_document.uri;
        let position = &params.text_document_position.position;
        let mut completion = Ok(None);

        let typ = {
            let renderer = self.renderer.lock().await;
            renderer.get_position_type(uri, position)
        };
        if let Some(typ) = typ {
            /// 添加额外参数
            fn add_extra_params(list: &mut Vec<CompletionItem>, uri: &Url) {
                for item in list {
                    if let Some(data) = &mut item.data {
                        if data.is_object() {
                            if let Some(data) = data.as_object_mut() {
                                data.insert("from_ts_server".to_string(), Value::Bool(true));
                                data.insert("original_uri".to_string(), json!(uri));
                            }
                        }
                    } else {
                        item.data = Some(json!({
                            "from_ts_server": true,
                            "original_uri": json!(uri)
                        }));
                    }
                }
            }
            /// 给每项的 data 中加入标记表示来自 ts 服务器的补全
            fn completion_add_flag(completion: &mut Result<Option<CompletionResponse>>, uri: &Url) {
                if let Ok(Some(completion)) = completion {
                    match completion {
                        CompletionResponse::Array(list) => {
                            add_extra_params(list, uri);
                        }
                        CompletionResponse::List(list) => {
                            add_extra_params(&mut list.items, uri);
                        }
                    }
                }
            }
            match typ {
                PositionType::Script => {
                    let uri = uri.clone();
                    debug!("lock ts_server await");
                    let ts_server = self.ts_server.read().await;
                    debug!("lock ts_server");
                    completion = ts_server.completion(params).await;
                    completion_add_flag(&mut completion, &uri);
                }
                PositionType::TemplateExpr(pos) => {
                    let mut params = params.clone();
                    params.text_document_position.position = pos;
                    debug!("lock ts_server await");
                    let ts_server = self.ts_server.read().await;
                    debug!("lock ts_server");
                    completion = ts_server.completion(params).await;
                    completion_add_flag(&mut completion, uri);
                }
                PositionType::Template => {
                    self.update_html_languageservice(uri).await;
                    let html_document = {
                        let renderer = self.renderer.lock().await;
                        renderer.get_html_document(uri)
                    };
                    if let Some(html_document) = html_document {
                        let document_context = DefaultDocumentContext {};
                        debug!("lock all await");
                        let data_manager = self.data_manager.lock().await;
                        let html_server = self.html_server.lock().await;
                        let text_documents = self.text_documents.lock().await;
                        debug!("lock all");
                        let text_document = text_documents.get_document(uri).unwrap();
                        let html_result = html_server
                            .do_complete(
                                text_document,
                                position,
                                &html_document,
                                document_context,
                                None,
                                &data_manager,
                            )
                            .await;
                        completion = Ok(Some(CompletionResponse::List(html_result)));
                    }
                }
            }
        }

        info!("done {:?}", start_time.elapsed());
        completion
    }

    #[instrument]
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
                .read()
                .await
                .completion_resolve(params, serde_json::from_value(original_uri).unwrap())
                .await
        } else {
            Ok(params)
        }
    }

    #[instrument]
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        if !VueLspServer::is_uri_valid(&params.text_document_position_params.text_document.uri) {
            return Ok(None);
        }
        info!("start");
        let start_time = time::Instant::now();
        let mut definition = Ok(None);
        let uri = &params.text_document_position_params.text_document.uri;
        let position = &params.text_document_position_params.position;
        self.update_html_languageservice(uri).await;

        let typ = {
            let renderer = self.renderer.lock().await;
            renderer.get_position_type(uri, position)
        };
        if let Some(typ) = typ {
            match typ {
                PositionType::Script => {
                    debug!("Script");
                    definition = self
                        .ts_server
                        .read()
                        .await
                        .goto_definition(params, false)
                        .await;
                }
                PositionType::TemplateExpr(pos) => {
                    debug!("TemplateExpr");
                    let mut params = params.clone();
                    params.text_document_position_params.position = pos;
                    definition = self
                        .ts_server
                        .read()
                        .await
                        .goto_definition(params, true)
                        .await;
                }
                PositionType::Template => {
                    debug!("Template");
                    let renderer = self.renderer.lock().await;
                    if let Some(html_document) = renderer.get_html_document(uri) {
                        let text_documents = self.text_documents.lock().await;
                        let text_document = text_documents.get_document(uri).unwrap();
                        let root =
                            html_document.find_root_at(text_document.offset_at(*position) as usize);

                        if let Some(root) = root {
                            if root.tag.as_ref().is_some_and(|tag| &tag[..] == "template") {
                                let offset = text_document.offset_at(*position) as usize;
                                if let Some(node) = html_document.find_node_at(offset, &mut vec![])
                                {
                                    let token_type = Node::find_token_type_in_node(&node, offset);
                                    if token_type == TokenType::StartTag
                                        || token_type == TokenType::EndTag
                                    {
                                        let tag = node.tag.as_ref().unwrap().clone();
                                        if let Some(location) =
                                            renderer.get_component_location(uri, &tag)
                                        {
                                            definition =
                                                Ok(Some(GotoDefinitionResponse::Scalar(location)));
                                        }
                                    }
                                }
                            } else {
                                debug!("(Vue2TsDecoratorServer/goto_definition) not in template");
                            }
                        }
                    }
                }
            }
        }

        info!("done {:?}", start_time.elapsed());
        definition
    }

    #[instrument]
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        if !VueLspServer::is_uri_valid(&params.text_document.uri) {
            return Ok(None);
        }
        info!("start");
        let start_time = time::Instant::now();
        let mut document_symbol_list = vec![];
        let uri = &params.text_document.uri;
        self.update_html_languageservice(uri).await;

        let html_document = {
            let renderer = self.renderer.lock().await;
            renderer.get_html_document(uri)
        };
        if let Some(html_document) = html_document {
            debug!("lock text_documents await");
            let text_documents = self.text_documents.lock().await;
            debug!("lock text_documents");
            let text_document = text_documents.get_document(uri).unwrap();

            document_symbol_list =
                HTMLLanguageService::find_document_symbols2(text_document, &html_document);
        }
        let script = document_symbol_list.iter_mut().find(|v| v.name == "script");
        if let Some(script) = script {
            debug!("lock ts_server await");
            let ts_server = self.ts_server.read().await;
            debug!("lock ts_server");
            let response = ts_server.document_symbol(params).await;
            if let Ok(Some(DocumentSymbolResponse::Nested(response))) = response {
                script.children = Some(response);
            }
        }
        info!("done {:?}", start_time.elapsed());
        Ok(Some(DocumentSymbolResponse::Nested(document_symbol_list)))
    }

    #[instrument]
    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        if !VueLspServer::is_uri_valid(&params.text_document.uri) {
            return Ok(None);
        }
        info!("start");
        let start_time = time::Instant::now();
        debug!("lock ts_server await");
        let ts_server = self.ts_server.read().await;
        debug!("lock ts_server");
        let result = ts_server.semantic_tokens_full(params).await;
        info!("done {:?}", start_time.elapsed());
        result
    }

    #[instrument]
    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        if !VueLspServer::is_uri_valid(&params.text_document.uri) {
            return Ok(None);
        }
        info!("start");
        let start_time = time::Instant::now();
        debug!("lock ts_server await");
        let ts_server = self.ts_server.read().await;
        debug!("lock ts_server");
        let result = ts_server.semantic_tokens_range(params).await;
        info!("done {:?}", start_time.elapsed());
        result
    }

    #[instrument]
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        if !VueLspServer::is_uri_valid(&params.text_document.uri) {
            return Ok(None);
        }
        info!("start");
        let start_time = time::Instant::now();
        debug!("lock ts_server await");
        let ts_server = self.ts_server.read().await;
        debug!("lock ts_server");
        let result = ts_server.code_action(params).await;
        info!("done {:?}", start_time.elapsed());
        result
    }

    async fn execute_command(&self, mut params: ExecuteCommandParams) -> Result<Option<Value>> {
        let text_documents = self.text_documents.lock().await;
        if params.command == "vue2-ts-decorator.restart.tsserver" {
            self.ts_server.write().await.restart(&text_documents).await;
            Ok(None)
        } else {
            params.command = params
                .command
                .replace("vue2-ts-decorator_typescript", "_typescript");
            self.ts_server.read().await.execute_command(params).await
        }
    }

    async fn shutdown(&self) -> Result<()> {
        self.ts_server.read().await.shutdown().await;
        Ok(())
    }
}
