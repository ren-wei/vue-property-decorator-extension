use std::sync::Arc;

use async_lsp_client::{LspServer, ServerMessage};
use lsp_textdocument::{FullTextDocument, TextDocuments};
use notification::{DidCloseTextDocument, Progress};
use request::{
    ApplyWorkspaceEdit, CodeActionRequest, Completion, DocumentSymbolRequest, ExecuteCommand,
    GotoDefinition, ResolveCompletionItem, SemanticTokensFullRequest, SemanticTokensRangeRequest,
    WillRenameFiles,
};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower_lsp::{jsonrpc, Client};
use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{
        notification::{
            DidChangeTextDocument, DidOpenTextDocument, LogMessage, Notification,
            PublishDiagnostics,
        },
        request::{HoverRequest, Request, WorkDoneProgressCreate, WorkspaceConfiguration},
        *,
    },
};
use tracing::debug;

use crate::convert::{ConvertBack, ConvertOptions, ConvertTo};
use crate::renderer::Mapping;
use crate::renderer::Renderer;

/// # TsServer
/// * 将请求转换格式后发送到 tsserver，然后将返回的响应转换为适合的格式
/// * 处理来自 tsserver 的请求和通知
pub struct TsServer {
    client: Client,
    server: LspServer,
    initialize_params: InitializeParams,
    renderer: Arc<Mutex<Renderer>>,
}

impl TsServer {
    pub fn new(client: Client, renderer: Arc<Mutex<Renderer>>) -> TsServer {
        let server = TsServer::spawn(client.clone(), Arc::clone(&renderer));
        TsServer {
            client,
            server,
            renderer,
            initialize_params: InitializeParams::default(),
        }
    }

    fn spawn(client: Client, renderer: Arc<Mutex<Renderer>>) -> LspServer {
        let exe_path = std::env::current_exe().unwrap();
        let mut path = exe_path.parent().unwrap().to_path_buf();
        while !path.file_name().is_some_and(|name| name == "server") {
            path = path.parent().unwrap().to_path_buf();
        }
        path.push("tsserver.mjs");

        let (mut server, mut rx) = LspServer::new("node", [path.to_str().unwrap(), "--stdio"]);
        let server_ = server.clone();

        tokio::spawn(async move {
            loop {
                if let Some(message) = rx.recv().await {
                    let mut renderer = renderer.lock().await;
                    TsServer::process_message(&client, &mut server, message, &mut renderer).await;
                } else {
                    break;
                }
            }
        });

        server_
    }

    /// 重启 ts 服务器
    pub async fn restart(&mut self, text_documents: &TextDocuments) {
        self.server.shutdown().await.unwrap();
        self.server.exit().await;
        let client = self.client.clone();
        let renderer = self.renderer.clone();
        let server = TsServer::spawn(client, renderer);
        self.server = server;
        self.server
            .initialize(self.initialize_params.clone())
            .await
            .unwrap();
        self.server.initialized().await;
        for (uri, document) in text_documents.documents() {
            self.did_open(uri, document).await;
        }
    }

    pub async fn initialize(&mut self, params: InitializeParams) -> Result<InitializeResult> {
        self.initialize_params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: params.root_uri.clone(),
            capabilities: params.capabilities.clone(),
            initialization_options: Some(json!({
                "locale": "zh-CN",
            })),
            ..params.clone()
        };
        self.server.initialize(params).await
    }

    pub async fn initialized(&self) {
        self.server.initialized().await;
    }

    pub async fn did_open(&mut self, uri: &Url, document: &FullTextDocument) {
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            renderer: Some(&renderer),
            ..Default::default()
        };
        let target_uri = uri.clone().convert_to(options).await;
        if Renderer::is_vue_component(uri) {
            if let Ok(document) = Renderer::get_document_from_file(&target_uri).await {
                self.server
                    .send_notification::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri: target_uri,
                            language_id: "typescript".to_string(),
                            version: document.version(),
                            text: document.get_content(None).to_string(),
                        },
                    })
                    .await;
            } else {
                self.server
                    .send_notification::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri: target_uri,
                            language_id: "typescript".to_string(),
                            version: document.version(),
                            text: document.get_content(None).to_string(),
                        },
                    })
                    .await
            }
        } else {
            self.server
                .send_notification::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: target_uri,
                        language_id: document.language_id().to_string(),
                        version: document.version(),
                        text: document.get_content(None).to_string(),
                    },
                })
                .await
        }
    }

    pub async fn did_change(
        &mut self,
        params: DidChangeTextDocumentParams,
        document: &FullTextDocument,
    ) {
        let uri = params.text_document.uri.clone();
        let mut renderer = self.renderer.lock().await;
        let params = renderer.update(&uri, params, document).await;
        let options = &ConvertOptions {
            renderer: Some(&renderer),
            ..Default::default()
        };
        let params = params.convert_to(options).await;
        drop(renderer);
        self.server
            .send_notification::<DidChangeTextDocument>(params)
            .await;
    }

    pub async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let renderer = self.renderer.lock().await;
        self.server
            .send_notification::<DidCloseTextDocument>(
                params
                    .convert_to(&ConvertOptions {
                        renderer: Some(&renderer),
                        ..Default::default()
                    })
                    .await,
            )
            .await
    }

    pub async fn did_save(&mut self, params: DidChangeTextDocumentParams) {
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            renderer: Some(&renderer),
            ..Default::default()
        };
        let params = params.convert_to(options).await;
        drop(renderer);
        self.server
            .send_notification::<DidChangeTextDocument>(params)
            .await;
    }

    pub async fn will_rename_files(
        &self,
        params: RenameFilesParams,
    ) -> Result<Option<WorkspaceEdit>> {
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            renderer: Some(&renderer),
            ..Default::default()
        };
        let response = self
            .server
            .send_request::<WillRenameFiles>(params.convert_to(options).await)
            .await;
        response.convert_back(options).await
    }

    pub async fn hover(&self, params: TextDocumentPositionParams) -> Result<Option<Hover>> {
        let uri = params.text_document.uri.clone();
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        let response = self
            .server
            .send_request::<HoverRequest>(HoverParams {
                text_document_position_params: params.convert_to(options).await,
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
            })
            .await;
        response.convert_back(options).await
    }

    pub async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        let params = params.convert_to(options).await;
        drop(renderer);

        debug!("send_request");
        let start_time = std::time::Instant::now();
        let result = self.server.send_request::<Completion>(params).await;
        debug!("request time: {:?}", start_time.elapsed());

        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        result.convert_back(options).await
    }

    pub async fn completion_resolve(
        &self,
        params: CompletionItem,
        original_uri: Url,
    ) -> Result<CompletionItem> {
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&original_uri),
            renderer: Some(&renderer),
        };
        let params = params.convert_to(options).await;
        drop(renderer);

        debug!("send_request");
        let start_time = std::time::Instant::now();
        let result = self
            .server
            .send_request::<ResolveCompletionItem>(params)
            .await;
        debug!("request time: {:?}", start_time.elapsed());

        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&original_uri),
            renderer: Some(&renderer),
        };

        // 处理 "this." 后面补全多了 "this." 的问题
        let mut result = result.convert_back(options).await?;
        if let Some(data) = &result.data {
            if data.is_object() {
                let data = data.as_object().unwrap();
                let line = data.get("line");
                let offset = data.get("offset");
                if line.is_some() && offset.is_some() {
                    let line = line.unwrap().as_number().unwrap().as_u64().unwrap() as u32 - 1;
                    let offset = offset.unwrap().as_number().unwrap().as_u64().unwrap() as u32 - 1;
                    let document = renderer.get_document(&original_uri).unwrap();
                    if offset >= 5 {
                        let text = document.get_content(Some(Range {
                            start: Position {
                                line,
                                character: offset - 5,
                            },
                            end: Position {
                                line,
                                character: offset,
                            },
                        }));
                        if text == "this."
                            && result
                                .insert_text
                                .as_ref()
                                .is_some_and(|v| v.starts_with("this."))
                        {
                            result.insert_text =
                                Some(result.insert_text.as_ref().unwrap()[5..].to_string());
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    pub async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
        is_in_template: bool,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();

        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        let params = params.convert_to(options).await;
        drop(renderer);

        debug!("send_request");
        let start_time = std::time::Instant::now();
        let response = self.server.send_request::<GotoDefinition>(params).await;
        debug!("request time: {:?}", start_time.elapsed());

        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        let response = response.convert_back(options).await?;

        if is_in_template {
            if let Some(response) = response {
                match response {
                    GotoDefinitionResponse::Array(array) => {
                        let mut result = vec![];
                        for item in array {
                            if !renderer.is_position_valid(&item.uri, &item.range.start) {
                                let response = self
                                    .server
                                    .send_request::<GotoDefinition>(GotoDefinitionParams {
                                        text_document_position_params: TextDocumentPositionParams {
                                            text_document: TextDocumentIdentifier {
                                                uri: item.uri.clone().convert_to(options).await,
                                            },
                                            position: item.range.start,
                                        },
                                        work_done_progress_params: WorkDoneProgressParams {
                                            work_done_token: None,
                                        },
                                        partial_result_params: PartialResultParams {
                                            partial_result_token: None,
                                        },
                                    })
                                    .await;
                                if let Ok(Some(GotoDefinitionResponse::Array(mut value))) =
                                    response.convert_back(options).await
                                {
                                    result.append(&mut value);
                                }
                            } else {
                                result.push(item);
                            }
                        }
                        return Ok(Some(GotoDefinitionResponse::Array(result)));
                    }
                    GotoDefinitionResponse::Link(link) => {
                        let mut result: Vec<LocationLink> = vec![];
                        for mut item in link {
                            let origin_selection_range = item.origin_selection_range;
                            if !renderer.is_position_valid(
                                &item.target_uri,
                                &item.target_selection_range.start,
                            ) {
                                let response = self
                                    .server
                                    .send_request::<GotoDefinition>(GotoDefinitionParams {
                                        text_document_position_params: TextDocumentPositionParams {
                                            text_document: TextDocumentIdentifier {
                                                uri: item.target_uri.convert_to(options).await,
                                            },
                                            position: item.target_selection_range.start,
                                        },
                                        work_done_progress_params: WorkDoneProgressParams {
                                            work_done_token: None,
                                        },
                                        partial_result_params: PartialResultParams {
                                            partial_result_token: None,
                                        },
                                    })
                                    .await;
                                if let Ok(Some(GotoDefinitionResponse::Link(mut value))) =
                                    response.convert_back(options).await
                                {
                                    for v in &mut value {
                                        // 重置 origin_selection_range 的值
                                        if let Some(origin_selection_range) = origin_selection_range
                                        {
                                            v.origin_selection_range = renderer
                                                .get_original_range(&uri, &origin_selection_range);
                                        }
                                    }

                                    for v in value {
                                        // 如果 result 中已经存在了相同的 target_range，则不再添加
                                        if result
                                            .iter()
                                            .find(|vv| {
                                                vv.target_uri == v.target_uri
                                                    && vv.target_range == v.target_range
                                            })
                                            .is_none()
                                        {
                                            result.push(v);
                                        }
                                    }
                                }
                            } else {
                                // 重置 origin_selection_range 的值
                                if let Some(origin_selection_range) = origin_selection_range {
                                    item.origin_selection_range =
                                        renderer.get_original_range(&uri, &origin_selection_range);
                                }
                                // 如果 result 中已经存在了相同的 target_range，则不再添加
                                if result
                                    .iter()
                                    .find(|v| {
                                        v.target_uri == item.target_uri
                                            && v.target_range == item.target_range
                                    })
                                    .is_none()
                                {
                                    result.push(item);
                                }
                            }
                        }
                        return Ok(Some(GotoDefinitionResponse::Link(result)));
                    }
                    GotoDefinitionResponse::Scalar(location) => {
                        if !renderer.is_position_valid(&location.uri, &location.range.start) {
                            return self
                                .server
                                .send_request::<GotoDefinition>(GotoDefinitionParams {
                                    text_document_position_params: TextDocumentPositionParams {
                                        text_document: TextDocumentIdentifier { uri: location.uri },
                                        position: location.range.start,
                                    },
                                    work_done_progress_params: WorkDoneProgressParams {
                                        work_done_token: None,
                                    },
                                    partial_result_params: PartialResultParams {
                                        partial_result_token: None,
                                    },
                                })
                                .await
                                .convert_back(options)
                                .await;
                        } else {
                            return Ok(Some(GotoDefinitionResponse::Scalar(location)));
                        }
                    }
                }
            }
        }
        Ok(response)
    }

    pub async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.clone();
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        let params = params.convert_to(options).await;
        drop(renderer);
        debug!("send_request");
        let start_time = std::time::Instant::now();

        let result = self
            .server
            .send_request::<DocumentSymbolRequest>(params)
            .await;
        debug!("request time: {:?}", start_time.elapsed());

        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        result.convert_back(options).await
    }

    pub async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.clone();
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        let params = params.convert_to(options).await;
        drop(renderer);

        debug!("send_request");
        let start_time = std::time::Instant::now();
        let result = self
            .server
            .send_request::<SemanticTokensFullRequest>(params)
            .await;
        debug!("request time: {:?}", start_time.elapsed());

        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        result.convert_back(options).await
    }

    pub async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri.clone();
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        let params = params.convert_to(options).await;
        drop(renderer);

        debug!("send_request");
        let start_time = std::time::Instant::now();
        let result = self
            .server
            .send_request::<SemanticTokensRangeRequest>(params)
            .await;
        debug!("request time: {:?}", start_time.elapsed());

        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        result.convert_back(options).await
    }

    pub async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let renderer = self.renderer.lock().await;
        let options = &ConvertOptions {
            uri: Some(&uri),
            renderer: Some(&renderer),
        };
        let params = params.convert_to(options).await;
        drop(renderer);

        debug!("send_request");
        let start_time = std::time::Instant::now();
        let response = self.server.send_request::<CodeActionRequest>(params).await;
        debug!("request time: {:?}", start_time.elapsed());

        response.convert_back(&ConvertOptions::default()).await
    }

    pub async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        self.server
            .send_request::<ExecuteCommand>(params.convert_to(&ConvertOptions::default()).await)
            .await
    }

    /// 处理来自服务器的消息
    async fn process_message(
        client: &Client,
        server: &mut LspServer,
        message: ServerMessage,
        renderer: &mut Renderer,
    ) {
        match message {
            ServerMessage::Notification(notification) => match &notification.method[..] {
                LogMessage::METHOD => {
                    let params: LogMessageParams =
                        serde_json::from_value(notification.params.unwrap()).unwrap();
                    client.log_message(params.typ, params.message).await;
                }
                PublishDiagnostics::METHOD => {
                    let params: PublishDiagnosticsParams =
                        serde_json::from_value(notification.params.unwrap()).unwrap();
                    let uri = &params
                        .uri
                        .clone()
                        .convert_back(&ConvertOptions {
                            renderer: Some(&renderer),
                            ..Default::default()
                        })
                        .await;
                    let diags = params
                        .diagnostics
                        .convert_back(&ConvertOptions {
                            uri: Some(&uri),
                            renderer: Some(&renderer),
                        })
                        .await;
                    client
                        .publish_diagnostics(uri.clone(), diags, params.version)
                        .await;
                }
                Progress::METHOD => {
                    let params: ProgressParams =
                        serde_json::from_value(notification.params.unwrap()).unwrap();
                    client.send_notification::<Progress>(params).await;
                }
                _ => {}
            },
            ServerMessage::Request(req) => {
                let id = req.id().unwrap().clone();
                match req.method() {
                    WorkspaceConfiguration::METHOD => {
                        server
                            .send_response::<WorkspaceConfiguration>(id, vec![])
                            .await
                    }
                    WorkDoneProgressCreate::METHOD => {
                        let params: WorkDoneProgressCreateParams =
                            serde_json::from_value(req.params().unwrap().clone()).unwrap();
                        client
                            .send_request::<WorkDoneProgressCreate>(params)
                            .await
                            .unwrap();
                        server.send_response::<WorkDoneProgressCreate>(id, ()).await;
                    }
                    ApplyWorkspaceEdit::METHOD => {
                        let params: ApplyWorkspaceEditParams =
                            serde_json::from_value(req.params().unwrap().clone()).unwrap();
                        let request_params = params
                            .convert_back(&ConvertOptions {
                                renderer: Some(renderer),
                                ..Default::default()
                            })
                            .await;

                        let response = client
                            .send_request::<ApplyWorkspaceEdit>(request_params)
                            .await
                            .unwrap();
                        server
                            .send_response::<ApplyWorkspaceEdit>(id, response)
                            .await;
                    }
                    _ => {
                        server
                            .send_error_response(
                                id,
                                jsonrpc::Error {
                                    code: jsonrpc::ErrorCode::MethodNotFound,
                                    message: std::borrow::Cow::Borrowed("Method Not Found"),
                                    data: req.params().cloned(),
                                },
                            )
                            .await;
                    }
                }
            }
        }
    }

    pub async fn shutdown(&self) {
        self.server.shutdown().await.unwrap();
        self.server.exit().await;
    }
}
