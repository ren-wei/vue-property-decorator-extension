use std::collections::HashMap;

use async_lsp_client::{LspServer, ServerMessage};
use html_languageservice::parser::html_document::HTMLDocument;
use lsp_textdocument::FullTextDocument;
use tokio::sync::mpsc::Sender;
use tower_lsp::{
    jsonrpc::{self, Result},
    lsp_types::{
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, LogMessage,
            Notification, PublishDiagnostics,
        },
        request::{ApplyWorkspaceEdit, Completion, HoverRequest, Request},
        ApplyWorkspaceEditParams, ChangeAnnotation, ChangeAnnotationIdentifier, CompletionParams,
        CompletionResponse, Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, DocumentChanges, Hover, HoverParams, InitializeParams,
        InitializeResult, InitializedParams, LogMessageParams,
        OptionalVersionedTextDocumentIdentifier, PublishDiagnosticsParams,
        TextDocumentContentChangeEvent, TextDocumentEdit, TextDocumentIdentifier, TextDocumentItem,
        TextDocumentPositionParams, TextEdit, Uri, VersionedTextDocumentIdentifier,
        WorkDoneProgressParams, WorkspaceEdit,
    },
    Client,
};
use tracing::warn;

use crate::{renderer, util};

/// # CssServer
/// 将请求转换格式后，发送到 css-lsp-server，并处理响应
pub struct CssServer {
    server: LspServer,
}

impl CssServer {
    pub fn new(client: Client, tx: Sender<(Uri, Option<i32>, Vec<Diagnostic>)>) -> CssServer {
        let server = CssServer::spawn(client, tx);
        CssServer { server }
    }

    fn spawn(client: Client, tx: Sender<(Uri, Option<i32>, Vec<Diagnostic>)>) -> LspServer {
        let exe_path = std::env::current_exe().unwrap();
        let mut path = exe_path.parent().unwrap().to_path_buf();
        while !path.file_name().is_some_and(|name| name == "server") {
            path = path.parent().unwrap().to_path_buf();
        }
        path.push("css-lsp-server.js");

        let (server, mut rx) = LspServer::new(
            "node",
            ["--enable-source-maps", path.to_str().unwrap(), "--stdio"],
        );
        let server_ = server.clone();

        tokio::spawn(async move {
            loop {
                if let Some(message) = rx.recv().await {
                    CssServer::process_message(&client, &server, message, &tx).await;
                } else {
                    break;
                }
            }
        });

        server_
    }

    pub async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.server
            .initialize(InitializeParams {
                process_id: Some(std::process::id()),
                capabilities: params.capabilities.clone(),
                workspace_folders: params.workspace_folders.clone(),
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
                ..params.clone()
            })
            .await
    }

    pub async fn initialized(&self) {
        self.server.initialized(InitializedParams {}).await;
    }

    pub async fn did_open(&self, params: DidOpenTextDocumentParams, html_document: &HTMLDocument) {
        let (content, suffix) = get_css_source(&params.text_document.text, html_document);
        self.server
            .send_notification::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: params.text_document.uri.convert_to(suffix),
                    language_id: suffix[1..].to_string(),
                    version: params.text_document.version,
                    text: content,
                },
            })
            .await;
    }

    pub async fn did_change(
        &self,
        params: DidChangeTextDocumentParams,
        document: &FullTextDocument,
        html_document: &HTMLDocument,
    ) {
        let (content, suffix) = get_css_source(&document.get_content(None), html_document);
        self.server
            .send_notification::<DidChangeTextDocument>(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: params.text_document.uri.convert_to(suffix),
                    version: params.text_document.version,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: content,
                }],
            })
            .await;
    }

    pub async fn did_close(
        &self,
        params: DidCloseTextDocumentParams,
        html_document: &HTMLDocument,
    ) {
        let uri = params.text_document.uri;
        let suffix = get_suffix_from_html(html_document);
        self.server
            .send_notification::<DidCloseTextDocument>(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: uri.convert_to(suffix),
                },
            })
            .await;
    }

    pub async fn hover(
        &self,
        params: HoverParams,
        html_document: &HTMLDocument,
    ) -> Result<Option<Hover>> {
        let suffix = get_suffix_from_html(html_document);
        self.server
            .send_request::<HoverRequest>(params.convert_to(suffix))
            .await
            .convert_back(suffix)
    }

    pub async fn completion(
        &self,
        params: CompletionParams,
        html_document: &HTMLDocument,
    ) -> Result<Option<CompletionResponse>> {
        let suffix = get_suffix_from_html(html_document);
        self.server
            .send_request::<Completion>(params.convert_to(suffix))
            .await
            .convert_back(suffix)
    }

    async fn process_message(
        client: &Client,
        server: &LspServer,
        message: ServerMessage,
        tx: &Sender<(Uri, Option<i32>, Vec<Diagnostic>)>,
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
                    let uri = params
                        .uri
                        .clone()
                        .convert_back(get_suffix_from_uri(&params.uri));
                    let diags = params
                        .diagnostics
                        .convert_back(get_suffix_from_uri(&params.uri));
                    tx.send((uri.clone(), params.version, diags)).await.unwrap();
                }
                _ => {}
            },
            ServerMessage::Request(req) => {
                let id = req.id().unwrap().clone();
                match req.method() {
                    ApplyWorkspaceEdit::METHOD => {
                        let params: ApplyWorkspaceEditParams =
                            serde_json::from_value(req.params().unwrap().clone()).unwrap();
                        let request_params = params.convert_back("");

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
}

fn get_suffix_from_uri(uri: &Uri) -> &'static str {
    if uri.as_str().ends_with(".css") {
        ".css"
    } else if uri.as_str().ends_with(".scss") {
        ".scss"
    } else if uri.as_str().ends_with(".less") {
        ".less"
    } else {
        warn!("Warning: unknown suffix for URI: {}", uri.as_str());
        ".css" // Default to ".css" if the suffix is unknown
    }
}

fn get_suffix_from_html(html_document: &HTMLDocument) -> &'static str {
    let style = html_document
        .roots
        .iter()
        .find(|v| v.tag.as_ref().is_some_and(|tag| tag == "style"));
    if let Some(style) = style {
        if let Some(v) = style.attributes.get("lang") {
            if v.value.as_ref().is_some_and(|v| v == "\"scss\"") {
                ".scss"
            } else if v.value.as_ref().is_some_and(|v| v == "\"less\"") {
                ".less"
            } else {
                ".css"
            }
        } else {
            ".css"
        }
    } else {
        ".css"
    }
}

/// 获取 vue 文件中的 css 部分，其余部分填充为空格或换行符
/// 返回转换后的内容和 文件后缀
fn get_css_source(source: &str, html_document: &HTMLDocument) -> (String, &'static str) {
    let style = html_document
        .roots
        .iter()
        .find(|v| v.tag.as_ref().is_some_and(|v| v == "style"));
    if let Some(style) = style {
        if style.start_tag_end.is_some() && style.end_tag_start.is_some() {
            let start_tag_end = style.start_tag_end.unwrap();
            let end_tag_start = style.end_tag_start.unwrap();
            let content = renderer::get_fill_space_source(source, start_tag_end, end_tag_start);
            (content, get_suffix_from_html(html_document))
        } else {
            ("".to_string(), ".css")
        }
    } else {
        ("".to_string(), ".css")
    }
}

trait ConvertTo {
    fn convert_to(self, suffix: &str) -> Self;
}

impl ConvertTo for Uri {
    fn convert_to(self, suffix: &str) -> Self {
        let uri_str = util::to_file_path_string(&self);
        util::create_uri_from_str(&format!("{}{}", uri_str, suffix))
    }
}

impl ConvertTo for TextDocumentPositionParams {
    fn convert_to(self, suffix: &str) -> Self {
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: self.text_document.uri.convert_to(suffix),
            },
            position: self.position,
        }
    }
}

impl ConvertTo for HoverParams {
    fn convert_to(self, suffix: &str) -> Self {
        HoverParams {
            text_document_position_params: self.text_document_position_params.convert_to(suffix),
            work_done_progress_params: self.work_done_progress_params,
        }
    }
}

impl ConvertTo for CompletionParams {
    fn convert_to(self, suffix: &str) -> Self {
        CompletionParams {
            text_document_position: self.text_document_position.convert_to(suffix),
            work_done_progress_params: self.work_done_progress_params,
            partial_result_params: self.partial_result_params,
            context: self.context,
        }
    }
}

trait ConvertBack {
    fn convert_back(self, suffix: &str) -> Self;
}

impl ConvertBack for Uri {
    fn convert_back(self, suffix: &str) -> Self {
        let uri_str = util::to_file_path_string(&self);
        if uri_str.ends_with(suffix) {
            util::create_uri_from_str(&uri_str[..uri_str.len() - suffix.len()])
        } else {
            self
        }
    }
}

impl ConvertBack for Vec<Diagnostic> {
    fn convert_back(self, _suffix: &str) -> Self {
        self
    }
}

impl<T: ConvertBack> ConvertBack for Result<T> {
    fn convert_back(self, suffix: &str) -> Self {
        Ok(self?.convert_back(suffix))
    }
}

impl<T: ConvertBack> ConvertBack for Option<T> {
    fn convert_back(self, suffix: &str) -> Self {
        Some(self?.convert_back(suffix))
    }
}

impl ConvertBack for HashMap<Uri, Vec<TextEdit>> {
    fn convert_back(self, suffix: &str) -> Self {
        let mut map = HashMap::new();
        for (key, value) in self {
            map.insert(key.convert_back(suffix), value);
        }
        map
    }
}

impl ConvertBack for DocumentChanges {
    fn convert_back(self, suffix: &str) -> Self {
        match self {
            DocumentChanges::Edits(edits) => {
                let mut result = vec![];
                for item in edits {
                    result.push(TextDocumentEdit {
                        text_document: OptionalVersionedTextDocumentIdentifier {
                            uri: item.text_document.uri.convert_back(suffix),
                            version: item.text_document.version,
                        },
                        edits: item.edits,
                    });
                }
                DocumentChanges::Edits(result)
            }
            DocumentChanges::Operations(operations) => DocumentChanges::Operations(operations),
        }
    }
}

impl ConvertBack for HashMap<ChangeAnnotationIdentifier, ChangeAnnotation> {
    fn convert_back(self, _suffix: &str) -> Self {
        self
    }
}

impl ConvertBack for ApplyWorkspaceEditParams {
    fn convert_back(self, suffix: &str) -> Self {
        ApplyWorkspaceEditParams {
            label: self.label,
            edit: WorkspaceEdit {
                changes: self.edit.changes.convert_back(suffix),
                document_changes: self.edit.document_changes.convert_back(suffix),
                change_annotations: self.edit.change_annotations.convert_back(suffix),
            },
        }
    }
}

impl ConvertBack for Hover {
    fn convert_back(self, _suffix: &str) -> Self {
        self
    }
}

impl ConvertBack for CompletionResponse {
    fn convert_back(self, _suffix: &str) -> Self {
        self
    }
}
