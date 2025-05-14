use std::str::FromStr;

use tower_lsp::lsp_types::*;

use crate::util;

use super::convert_options::ConvertOptions;

pub trait ConvertTo {
    async fn convert_to(self, options: &ConvertOptions) -> Self;
}

impl ConvertTo for Uri {
    /// 必须 root_uri, target_uri
    async fn convert_to(mut self, options: &ConvertOptions<'_>) -> Self {
        let (root_uri, target_uri) = options.root_uri_target_uri();

        let src_path = util::to_file_path(&self);
        let src_dir = util::to_file_path(root_uri);
        let dest_dir = util::to_file_path(target_uri);
        // 计算相对路径
        let rel_path = src_path
            .strip_prefix(&format!("{}/", src_dir.to_string_lossy()))
            .unwrap();
        // 转换为目标路径
        let dest_path = dest_dir.join(&rel_path);
        if dest_path.to_string_lossy().ends_with(".vue") {
            self = util::create_uri_from_str(&format!("{}{}", dest_path.to_string_lossy(), ".ts"));
        } else {
            self = util::create_uri_from_path(&dest_path);
        }
        self
    }
}

impl ConvertTo for Range {
    /// 必须 uri, renderer
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        if let Some(start) = renderer.get_mapping_position(uri, &self.start) {
            if let Some(end) = renderer.get_mapping_position(uri, &self.end) {
                return Range { start, end };
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
        if let Some(pos) = renderer.get_mapping_position(uri, &self.position) {
            position = pos;
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

impl ConvertTo for ReferenceParams {
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        ReferenceParams {
            text_document_position: self.text_document_position.convert_to(options).await,
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
            uri: Uri::from_str(&self.uri)
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
            let old_uri = Uri::from_str(&file.old_uri)
                .unwrap()
                .convert_to(options)
                .await;
            let new_uri = Uri::from_str(&file.new_uri)
                .unwrap()
                .convert_to(options)
                .await;
            files.push(FileRename {
                old_uri: old_uri.to_string(),
                new_uri: new_uri.to_string(),
            });
        }
        RenameFilesParams { files }
    }
}

impl ConvertTo for FileDelete {
    /// 必须 root_uri, target_uri
    async fn convert_to(self, options: &ConvertOptions<'_>) -> Self {
        FileDelete {
            uri: Uri::from_str(&self.uri)
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
                .replace("vue-property-decorator-extension_typescript", "_typescript"),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp::lsp_types::Uri;

    use crate::{convert::ConvertOptions, renderer::Renderer};

    use super::ConvertTo;

    async fn assert_uri(uri: &str, expected: &str) {
        let mut renderer = Renderer::new();
        renderer.set_root_uri_target_uri(
            Uri::from_str("file:///home/user/project").unwrap(),
            Uri::from_str("file:///home/user/.~$project").unwrap(),
        );
        let result = Uri::from_str(uri)
            .unwrap()
            .convert_to(&ConvertOptions {
                uri: Some(&Uri::from_str(uri).unwrap()),
                renderer: Some(&renderer),
            })
            .await
            .to_string();
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn convert_uri() {
        assert_uri(
            "file:///home/user/project/src/a.vue",
            "file:///home/user/.~%24project/src/a.vue.ts",
        )
        .await;
        assert_uri(
            "file:///home/user/project/src/a.ts",
            "file:///home/user/.~%24project/src/a.ts",
        )
        .await;
    }
}
