use std::{path::PathBuf, str::FromStr};

use tower_lsp::lsp_types::*;

use super::convert_options::ConvertOptions;

pub trait ConvertTo {
    async fn convert_to(self, options: &ConvertOptions) -> Self;
}

impl ConvertTo for Url {
    /// 必须 root_uri, target_uri
    async fn convert_to(mut self, options: &ConvertOptions<'_>) -> Self {
        let (root_uri, target_uri) = options.root_uri_target_uri();

        let src_path = self.path();
        let src_dir = root_uri.path();
        let dest_dir = PathBuf::from_str(target_uri.path()).unwrap();
        // 计算相对路径
        let rel_path = src_path.strip_prefix(&format!("{}/", src_dir)).unwrap();
        // 转换为目标路径
        let dest_path = dest_dir.join(&rel_path);
        if rel_path.ends_with(".vue") {
            self.set_path(&format!("{}{}", dest_path.to_str().unwrap(), ".ts"));
        } else {
            self.set_path(dest_path.to_str().unwrap());
        }
        self
    }
}

impl ConvertTo for Range {
    /// 必须 uri, renderer
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        if let Some(document) = renderer.get_document(uri) {
            if let Some(start) =
                renderer.get_mapping_position(uri, document.offset_at(self.start) as usize)
            {
                if let Some(end) =
                    renderer.get_mapping_position(uri, document.offset_at(self.end) as usize)
                {
                    return Range { start, end };
                }
            }
        }
        self
    }
}

impl ConvertTo for TextDocumentPositionParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        let mut position = self.position;
        if let Some(document) = renderer.get_document(uri) {
            if let Some(pos) =
                renderer.get_mapping_position(uri, document.offset_at(self.position) as usize)
            {
                position = pos;
            }
        }
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: self.text_document.uri.convert_to(options).await,
            },
            position,
        }
    }
}

impl ConvertTo for TextDocumentIdentifier {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        TextDocumentIdentifier {
            uri: self.uri.convert_to(options).await,
        }
    }
}

impl ConvertTo for VersionedTextDocumentIdentifier {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        VersionedTextDocumentIdentifier {
            uri: self.uri.convert_to(options).await,
            ..self
        }
    }
}

impl ConvertTo for TextDocumentItem {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        TextDocumentItem {
            uri: self.uri.convert_to(options).await,
            language_id: "typescript".to_string(),
            ..self
        }
    }
}

impl ConvertTo for DidChangeTextDocumentParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        DidChangeTextDocumentParams {
            text_document: self.text_document.convert_to(options).await,
            ..self
        }
    }
}

impl ConvertTo for DidCloseTextDocumentParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        DidCloseTextDocumentParams {
            text_document: self.text_document.convert_to(options).await,
        }
    }
}

impl ConvertTo for CompletionParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        CompletionParams {
            text_document_position: self.text_document_position.convert_to(options).await,
            ..self
        }
    }
}

impl ConvertTo for TextEdit {
    /// 必须 uri, renderer
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        TextEdit {
            range: self.range.convert_to(options).await,
            ..self
        }
    }
}

impl ConvertTo for InsertReplaceEdit {
    /// 必须 uri, renderer
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        InsertReplaceEdit {
            insert: self.insert.convert_to(options).await,
            replace: self.replace.convert_to(options).await,
            ..self
        }
    }
}

impl ConvertTo for Option<CompletionTextEdit> {
    /// 必须 uri, renderer
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        Some(match self? {
            CompletionTextEdit::Edit(edit) => {
                CompletionTextEdit::Edit(edit.convert_to(options).await)
            }
            CompletionTextEdit::InsertAndReplace(insert) => {
                CompletionTextEdit::InsertAndReplace(insert.convert_to(options).await)
            }
        })
    }
}

impl ConvertTo for CompletionItem {
    /// 必须 uri, renderer
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        let text_edit = self.text_edit.convert_to(options).await;
        CompletionItem { text_edit, ..self }
    }
}

impl ConvertTo for GotoDefinitionParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        GotoDefinitionParams {
            text_document_position_params: self
                .text_document_position_params
                .convert_to(options)
                .await,
            ..self
        }
    }
}

impl ConvertTo for DocumentSymbolParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        DocumentSymbolParams {
            text_document: self.text_document.convert_to(options).await,
            ..self
        }
    }
}

impl ConvertTo for SemanticTokensParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        SemanticTokensParams {
            text_document: self.text_document.convert_to(options).await,
            ..self
        }
    }
}

impl ConvertTo for SemanticTokensRangeParams {
    /// 必须 uri, root_uri, target_uri, renderer
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        let start = renderer.start_position(uri);
        let range = if let Some(start) = start {
            Range {
                start,
                end: renderer.end_position(uri).unwrap(),
            }
        } else {
            self.range
        };
        SemanticTokensRangeParams {
            work_done_progress_params: self.work_done_progress_params,
            partial_result_params: self.partial_result_params,
            text_document: TextDocumentIdentifier {
                uri: self.text_document.uri.convert_to(options).await,
            },
            range,
        }
    }
}

impl ConvertTo for FileCreate {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        FileCreate {
            uri: Url::from_str(&self.uri)
                .unwrap()
                .convert_to(options)
                .await
                .to_string(),
        }
    }
}

impl ConvertTo for CreateFilesParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        let mut files = vec![];
        for file in self.files {
            files.push(file.convert_to(options).await);
        }
        CreateFilesParams { files }
    }
}

impl ConvertTo for RenameFilesParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        let mut files = vec![];
        for file in self.files {
            files.push(FileRename {
                old_uri: Url::from_str(&file.old_uri)
                    .unwrap()
                    .convert_to(options)
                    .await
                    .to_string(),
                new_uri: Url::from_str(&file.new_uri)
                    .unwrap()
                    .convert_to(options)
                    .await
                    .to_string(),
            });
        }
        RenameFilesParams { files }
    }
}

impl ConvertTo for FileDelete {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        FileDelete {
            uri: Url::from_str(&self.uri)
                .unwrap()
                .convert_to(options)
                .await
                .to_string(),
        }
    }
}

impl ConvertTo for DeleteFilesParams {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        let mut files = vec![];
        for file in self.files {
            files.push(file.convert_to(options).await);
        }
        DeleteFilesParams { files }
    }
}

impl ConvertTo for CodeActionParams {
    /// 必须 uri, root_uri, target_uri, renderer
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        CodeActionParams {
            text_document: self.text_document.convert_to(options).await,
            range: self.range.convert_to(options).await,
            ..self
        }
    }
}

impl ConvertTo for ExecuteCommandParams {
    async fn convert_to(self, _: &ConvertOptions<'_>) -> Self {
        ExecuteCommandParams {
            command: self
                .command
                .replace("vue2-ts-decorator_typescript", "_typescript"),
            ..self
        }
    }
}
