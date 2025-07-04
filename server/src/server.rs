use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use core::fmt::Debug;
use html_languageservice::html_data::HTMLDataV1;
use html_languageservice::language_facts::data_provider::{HTMLDataProvider, IHTMLDataProvider};
use html_languageservice::parser::html_document::Node;
use html_languageservice::parser::html_scanner::TokenType;
use html_languageservice::{
    DefaultDocumentContext, HTMLDataManager, HTMLLanguageService, HTMLLanguageServiceOptions,
};
use lsp_textdocument::TextDocuments;
use serde_json::{json, Value};
use std::time;
use tokio::join;
use tokio::sync::{Mutex, RwLock};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::DidChangeConfiguration;
use tower_lsp::lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification,
};
use tower_lsp::lsp_types::request::WorkDoneProgressCreate;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tracing::{debug, info, instrument};

use crate::css_server::CssServer;
use crate::diagnostics::DiagnosticsManager;
use crate::renderer::{PositionType, Renderer};
use crate::ts_server::TsServer;
use crate::util;
use crate::vue_data::VueDataProvider;

pub struct VueLspServer {
    is_shared: bool,
    client: Client,
    text_documents: Arc<RwLock<TextDocuments>>,
    data_manager: Mutex<HTMLDataManager>,
    _diagnostics: DiagnosticsManager,
    html_server: Mutex<HTMLLanguageService>,
    ts_server: RwLock<TsServer>,
    css_server: CssServer,
    renderer: Arc<Mutex<Renderer>>,
    vue_data_provider: VueDataProvider,
    custom_data: StdMutex<Option<HTMLDataV1>>,
}

impl VueLspServer {
    pub fn new(
        client: Client,
        shared_text_documents: Option<Arc<RwLock<TextDocuments>>>,
    ) -> VueLspServer {
        let is_shared;
        let text_documents = if let Some(shared_text_documents) = shared_text_documents {
            is_shared = true;
            shared_text_documents
        } else {
            is_shared = false;
            Arc::new(RwLock::new(TextDocuments::new()))
        };
        let mut diagnostics = DiagnosticsManager::new(client.clone());
        let renderer = Arc::new(Mutex::new(Renderer::new()));
        let ts_server = RwLock::new(TsServer::new(
            client.clone(),
            Arc::clone(&renderer),
            diagnostics.register(),
        ));
        let html_server = HTMLLanguageService::new(&HTMLLanguageServiceOptions {
            case_sensitive: Some(true),
            ..Default::default()
        });
        let data_manager = Mutex::new(html_server.create_data_manager(true, None));
        let html_server = Mutex::new(html_server);
        let vue_data_provider = VueDataProvider::new();
        let custom_data = StdMutex::new(None);
        let css_server = CssServer::new(client.clone(), diagnostics.register());
        VueLspServer {
            is_shared,
            client,
            text_documents,
            data_manager,
            _diagnostics: diagnostics,
            html_server,
            ts_server,
            css_server,
            renderer,
            vue_data_provider,
            custom_data,
        }
    }

    /// 在进行 html 服务器相关的操作前调用
    async fn update_html_languageservice(&self, uri: &Uri) {
        debug!("(Vue2TsDecoratorServer/update_html_languageservice)");
        let tags_provider = {
            let mut renderer = self.renderer.lock().await;
            renderer.get_tags_provider(uri).await
        };

        let mut data_manager = self.data_manager.lock().await;
        let mut providers: Vec<Box<dyn IHTMLDataProvider>> = vec![
            Box::new(self.vue_data_provider.clone()),
            Box::new(tags_provider.clone()),
        ];
        let custom_data = { self.custom_data.lock().unwrap().clone() };
        if let Some(custom_data) = custom_data {
            providers.push(Box::new(HTMLDataProvider::new(
                "custom".to_string(),
                custom_data,
                true,
            )));
        }
        data_manager.set_data_providers(true, providers);

        let mut html_server = self.html_server.lock().await;
        html_server.set_completion_participants(vec![Box::new(tags_provider)]);
        debug!("(Vue2TsDecoratorServer/update_html_languageservice) done");
    }

    async fn get_configure(&self) {
        let custom_data = self
            .client
            .configuration(vec![ConfigurationItem {
                scope_uri: None,
                section: Some("vue-property-decorator.html.data".to_string()),
            }])
            .await
            .unwrap();
        if custom_data[0].is_object() {
            if custom_data[0].as_object().unwrap().is_empty() {
                *self.custom_data.lock().unwrap() = None;
                return;
            }
        } else {
            *self.custom_data.lock().unwrap() = None;
            self.client
                .show_message(
                    MessageType::WARNING,
                    "Parse configuration `vue-property-decorator.html.data` error: not object"
                        .to_string(),
                )
                .await;
            return;
        }
        match serde_json::from_value::<HTMLDataV1>(custom_data[0].clone()) {
            Ok(custom_data) => {
                *self.custom_data.lock().unwrap() = Some(custom_data);
            }
            Err(e) => {
                self.client
                    .show_message(
                        MessageType::WARNING,
                        format!(
                            "Parse configuration `vue-property-decorator.html.data` error: {}",
                            e
                        ),
                    )
                    .await;
            }
        }
    }

    /// 是否处理 uri
    fn is_uri_valid(uri: &Uri) -> bool {
        !util::to_file_path_string(uri).contains("/node_modules/")
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
        if let Some(folders) = &params.workspace_folders {
            if folders.len() != 1 {
                return Ok(InitializeResult {
                    server_info: Some(ServerInfo {
                        name: "vue-property-decorator-extension-server".to_string(),
                        version: Some("1.0.0".to_string()),
                    }),
                    ..Default::default()
                });
            }
            let root_uri = &folders[0].uri;
            self.renderer
                .lock()
                .await
                .init(
                    root_uri,
                    &self.client,
                    params
                        .work_done_progress_params
                        .work_done_token
                        .clone()
                        .unwrap(),
                )
                .await;
            self.css_server.initialize(params.clone()).await.unwrap();
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
            let mut commands = vec![
                "vue-property-decorator-extension.restart.tsserver".to_string(),
                "vue-property-decorator-extension.clean.cache.and.restart".to_string(),
            ];
            if let Some(execute_command_provider) = result.capabilities.execute_command_provider {
                let mut ts_commands = execute_command_provider
                    .commands
                    .iter()
                    .map(|c| {
                        c.replace("_typescript", "vue-property-decorator-extension_typescript")
                    })
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
                    references_provider: result.capabilities.references_provider,
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
                            will_rename: file_operation.clone(),
                            ..Default::default()
                        }),
                    }),
                    execute_command_provider: Some(ExecuteCommandOptions {
                        commands,
                        work_done_progress_options: WorkDoneProgressOptions {
                            work_done_progress: Some(true),
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
        self.css_server.initialized().await;
        self.ts_server.read().await.initialized().await;
        self.get_configure().await;
        self.client
            .register_capability(vec![Registration {
                id: "vue-property-decorator-extension".to_string(),
                method: DidChangeConfiguration::METHOD.to_string(),
                register_options: None,
            }])
            .await
            .unwrap();
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
            let mut text_documents = self.text_documents.write().await;
            text_documents.listen(
                DidOpenTextDocument::METHOD,
                &serde_json::to_value(&params).unwrap(),
            );
        }
        let uri = params.text_document.uri.clone();
        {
            self.renderer.lock().await.did_open(&uri).await;
        }
        join!(
            async {
                loop {
                    let is_wait = {
                        let renderer = self.renderer.lock().await;
                        renderer.is_wait_create(&uri)
                    };
                    if is_wait {
                        tokio::task::yield_now().await;
                    } else {
                        break;
                    }
                }
                debug!("did_open:lock text_documents await");
                let text_documents = self.text_documents.read().await;
                debug!("did_open:lock text_documents");
                let document = text_documents.get_document(&uri).unwrap();
                debug!("did_open:lock ts_server await");
                let ts_server = self.ts_server.read().await;
                debug!("did_open:lock ts_server");
                ts_server.did_open(&uri, document).await;
                info!("did_open:done {:?}", start_time.elapsed());
            },
            async {
                let html_document = { self.renderer.lock().await.get_html_document(&uri) };
                if let Some(html_document) = html_document {
                    self.css_server.did_open(params, &html_document).await;
                }
            }
        );
    }

    #[instrument]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if !VueLspServer::is_uri_valid(&params.text_document.uri) {
            return;
        }
        info!("start");
        let start_time = time::Instant::now();
        if !self.is_shared {
            let mut text_documents = self.text_documents.write().await;
            text_documents.listen(
                DidChangeTextDocument::METHOD,
                &serde_json::to_value(&params).unwrap(),
            );
        }
        let uri = params.text_document.uri.clone();
        let text_documents = self.text_documents.read().await;
        let document = text_documents.get_document(&uri).unwrap();
        let css_params = params.clone();
        join!(
            async {
                debug!("lock ts_server await");
                let ts_server = self.ts_server.read().await;
                debug!("lock ts_server");
                ts_server.did_change(params, &document).await;
            },
            async {
                let html_document = { self.renderer.lock().await.get_html_document(&uri) };
                if let Some(html_document) = html_document {
                    self.css_server
                        .did_change(css_params, &document, &html_document)
                        .await;
                }
            }
        );

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
            let mut text_documents = self.text_documents.write().await;
            text_documents.listen(
                DidCloseTextDocument::METHOD,
                &serde_json::to_value(&params).unwrap(),
            );
        }
        let css_params = params.clone();
        join!(
            async {
                self.ts_server.read().await.did_close(params).await;
            },
            async {
                let uri = css_params.text_document.uri.clone();
                let html_document = { self.renderer.lock().await.get_html_document(&uri) };
                if let Some(html_document) = html_document {
                    self.css_server.did_close(css_params, &html_document).await;
                }
            }
        );
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
            let ts_server = self.ts_server.read().await;
            debug!("lock ts_server");
            ts_server.did_save(change).await;
        }
        self.client.semantic_tokens_refresh().await.unwrap();
        info!("done {:?}", start_time.elapsed());
    }

    async fn will_create_files(&self, params: CreateFilesParams) -> Result<Option<WorkspaceEdit>> {
        let mut uris = vec![];
        for file in params.files {
            let uri = Uri::from_str(&file.uri).unwrap();
            uris.push(uri);
        }
        self.renderer.lock().await.will_create_files(uris);
        Ok(None)
    }

    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        {
            let mut uris = vec![];
            for file in &params.files {
                let uri = Uri::from_str(&file.new_uri).unwrap();
                uris.push(uri);
            }
            self.renderer.lock().await.will_create_files(uris);
        }
        let response = self.ts_server.read().await.will_rename_files(params).await;
        response
    }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        self.get_configure().await;
    }

    /// 监听到文件变更
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut did_create_files = vec![];
        let mut did_delete_files = vec![];
        let mut did_change_files = vec![];
        let text_documents = self.text_documents.read().await;
        for file in params.changes {
            match file.typ {
                FileChangeType::CREATED => {
                    if Renderer::is_uri_valid(&file.uri) {
                        did_create_files.push(file.uri);
                    }
                }
                FileChangeType::DELETED => {
                    if Renderer::is_uri_valid(&file.uri) {
                        did_delete_files.push(file.uri);
                    }
                }
                FileChangeType::CHANGED => {
                    if Renderer::is_uri_valid(&file.uri)
                        && text_documents.get_document(&file.uri).is_none()
                    {
                        did_change_files.push(file.uri);
                    }
                }
                _ => {}
            }
        }
        drop(text_documents);

        let mut renderer = self.renderer.lock().await;
        if did_create_files.len() > 0 {
            renderer.did_create_files(did_create_files).await;
        }
        if did_delete_files.len() > 0 {
            renderer.did_delete_files(did_delete_files);
        }
        if did_change_files.len() > 0 {
            for uri in did_change_files {
                renderer.save(&uri).await;
            }
        }
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
                        let text_documents = self.text_documents.read().await;
                        if let Some(text_document) = text_documents.get_document(uri) {
                            hover = Ok(html_server.do_hover(
                                text_document,
                                position,
                                &html_document,
                                None,
                                &data_manager,
                            ));
                        }
                    }
                }
                PositionType::Style => {
                    info!("In style");
                    let html_document = {
                        let renderer = self.renderer.lock().await;
                        renderer.get_html_document(uri)
                    };
                    if let Some(html_document) = html_document {
                        hover = self.css_server.hover(params, &html_document).await;
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
            fn add_extra_params(list: &mut Vec<CompletionItem>, uri: &Uri) {
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
            fn completion_add_flag(completion: &mut Result<Option<CompletionResponse>>, uri: &Uri) {
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
                        let text_documents = self.text_documents.read().await;
                        debug!("lock all");
                        let text_document = text_documents.get_document(uri).unwrap();
                        let html_result = html_server.do_complete(
                            text_document,
                            position,
                            &html_document,
                            document_context,
                            None,
                            &data_manager,
                        );
                        completion = Ok(Some(CompletionResponse::List(html_result)));
                    }
                }
                PositionType::Style => {
                    let html_document = {
                        let renderer = self.renderer.lock().await;
                        renderer.get_html_document(uri)
                    };
                    if let Some(html_document) = html_document {
                        completion = self.css_server.completion(params, &html_document).await;
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
                    let html_document = {
                        let renderer = self.renderer.lock().await;
                        renderer.get_html_document(uri)
                    };
                    if let Some(html_document) = html_document {
                        let text_documents = self.text_documents.read().await;
                        let text_document = text_documents.get_document(uri).unwrap();
                        let root =
                            html_document.find_root_at(text_document.offset_at(*position) as usize);

                        if let Some(root) = root {
                            if root.tag.as_ref().is_some_and(|tag| &tag[..] == "template") {
                                let offset = text_document.offset_at(*position) as usize;
                                drop(text_documents);
                                if let Some(node) = html_document.find_node_at(offset, &mut vec![])
                                {
                                    let token_type = Node::find_token_type_in_node(&node, offset);
                                    let renderer = self.renderer.lock().await;
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
                                    } else {
                                        let tag = node.tag.as_ref().unwrap().clone();
                                        let mut attr = None;
                                        for (attr_name, node_attr) in &node.attributes {
                                            if node_attr.offset <= offset
                                                && offset < node_attr.offset + attr_name.len()
                                            {
                                                attr = Some(attr_name.clone());
                                            }
                                        }
                                        if let Some(attr) = attr {
                                            let location = renderer
                                                .get_component_prop_location(uri, &tag, &attr);
                                            if let Some(location) = location {
                                                definition = Ok(Some(
                                                    GotoDefinitionResponse::Scalar(location),
                                                ));
                                            }
                                        };
                                    }
                                }
                            } else {
                                debug!("(Vue2TsDecoratorServer/goto_definition) not in template");
                            }
                        }
                    }
                }
                PositionType::Style => {}
            }
        }

        info!("done {:?}", start_time.elapsed());
        definition
    }

    #[instrument]
    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        if !VueLspServer::is_uri_valid(&params.text_document_position.text_document.uri) {
            return Ok(None);
        }
        info!("start");
        let start_time = time::Instant::now();
        debug!("lock ts_server await");
        let ts_server = self.ts_server.read().await;
        debug!("lock ts_server");
        let result = ts_server.references(params).await;
        info!("done {:?}", start_time.elapsed());
        result
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
            let text_documents = self.text_documents.read().await;
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
        let text_documents = self.text_documents.read().await;
        if params.command == "vue-property-decorator-extension.restart.tsserver" {
            self.ts_server.write().await.restart(&text_documents).await;
            Ok(None)
        } else if params.command == "vue-property-decorator-extension.clean.cache.and.restart" {
            let token = if let Some(token) = params.work_done_progress_params.work_done_token {
                token
            } else {
                let token = NumberOrString::String("clean-cache-and.restart".to_string());
                self.client
                    .send_request::<WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
                        token: token.clone(),
                    })
                    .await
                    .unwrap();
                token
            };
            self.renderer
                .lock()
                .await
                .clean_cache_and_restart(&self.client, token)
                .await;
            self.ts_server.write().await.restart(&text_documents).await;
            Ok(None)
        } else {
            params.command = params
                .command
                .replace("vue-property-decorator-extension_typescript", "_typescript");
            self.ts_server.read().await.execute_command(params).await
        }
    }

    async fn shutdown(&self) -> Result<()> {
        self.ts_server.read().await.shutdown().await;
        Ok(())
    }
}
