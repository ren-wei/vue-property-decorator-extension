use std::{collections::HashMap, path::PathBuf, str::FromStr};

use lsp_textdocument::FullTextDocument;
use tower_lsp::lsp_types::*;

use crate::{lazy::REG_TYPESCRIPT_MODULE, renderer::Renderer};

use super::convert_options::ConvertOptions;

pub trait ConvertBack {
    /// 将 ts 服务器上的请求或响应参数转换为实际项目的参数
    ///
    /// @param uri 当前请求的 uri
    async fn convert_back(self, options: &ConvertOptions) -> Self;
}

impl ConvertBack for Url {
    /// 必须 root_uri, target_uri
    async fn convert_back(mut self, options: &ConvertOptions<'_>) -> Self {
        let (root_uri, target_uri) = options.root_uri_target_uri();
        let dest_path = self.to_file_path().unwrap();
        let src_dir = root_uri.to_file_path().unwrap();
        let dest_dir = target_uri.to_file_path().unwrap();
        // 计算相对路径
        if let Ok(rel_path) = dest_path.strip_prefix(dest_dir.as_path()) {
            // 转换为原路径
            let src_path = src_dir.join(&rel_path);
            let mut src_path = src_path.to_str().unwrap();
            // 移除 .ts 扩展名
            if dest_path.to_str().unwrap().ends_with(".vue.ts") {
                src_path = &src_path[..src_path.len() - 3]; // .ts 总是3个字符
            }
            self.set_path(src_path);
        }
        self
    }
}

impl<T: ConvertBack, E> ConvertBack for Result<T, E> {
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        Ok(self?.convert_back(options).await)
    }
}

impl<T: ConvertBack> ConvertBack for Option<T> {
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        Some(self?.convert_back(options).await)
    }
}

impl<T: ConvertBack> ConvertBack for Vec<T> {
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let mut result = vec![];
        for item in self {
            result.push(item.convert_back(options).await);
        }
        result
    }
}

impl<A: ConvertBack, B: ConvertBack> ConvertBack for OneOf<A, B> {
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            OneOf::Left(left) => OneOf::Left(left.convert_back(options).await),
            OneOf::Right(right) => OneOf::Right(right.convert_back(options).await),
        }
    }
}

impl ConvertBack for HoverContents {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let (root_uri, target_uri) = options.root_uri_target_uri();
        fn convert_back_module(s: String, root_uri: &Url, target_uri: &Url) -> String {
            if s.contains("```typescript\nmodule") {
                if let Some(caps) = REG_TYPESCRIPT_MODULE.captures(&s) {
                    let src_dir = PathBuf::from_str(root_uri.path()).unwrap();
                    let dest_dir = target_uri.path();
                    let dest_path = caps.get(1).map_or("", |m| m.as_str());
                    if dest_path != "*.vue" && !dest_path.contains("/node_modules/") {
                        let rel_path =
                            if let Some(v) = dest_path.strip_prefix(&format!("{}/", dest_dir)) {
                                v
                            } else {
                                panic!("dest_path: {} dest_dir: {}", dest_path, dest_dir);
                            };
                        let src_path = src_dir.join(&rel_path);
                        let src_path = src_path.to_str().unwrap();
                        format!("\n```typescript\nmodule \"{}\"\n```\n", src_path)
                    } else {
                        s
                    }
                } else {
                    s
                }
            } else {
                s
            }
        }
        match self {
            HoverContents::Array(array) => {
                let mut result = vec![];
                for item in array {
                    if let MarkedString::String(item) = item {
                        result.push(MarkedString::String(convert_back_module(
                            item, root_uri, target_uri,
                        )));
                    } else {
                        result.push(item);
                    }
                }
                HoverContents::Array(result)
            }
            HoverContents::Markup(markup) => HoverContents::Markup(MarkupContent {
                kind: markup.kind,
                value: convert_back_module(markup.value, root_uri, target_uri),
            }),
            HoverContents::Scalar(scalar) => {
                if let MarkedString::String(scalar) = scalar {
                    HoverContents::Scalar(MarkedString::String(convert_back_module(
                        scalar, root_uri, target_uri,
                    )))
                } else {
                    HoverContents::Scalar(scalar)
                }
            }
        }
    }
}

impl ConvertBack for Hover {
    /// 必须 uri, root_uri, target_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        if let Some(range) = self.range {
            if let Some(range) = renderer.get_original_range(uri, &range) {
                return Hover {
                    contents: self.contents,
                    range: Some(range),
                };
            } else {
                return Hover {
                    contents: self.contents.convert_back(options).await,
                    range: Some(range),
                };
            }
        }
        self
    }
}

impl ConvertBack for TextEdit {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        let range = renderer.get_original_range(uri, &self.range);
        if let Some(range) = range {
            return TextEdit {
                range,
                new_text: self.new_text,
            };
        }
        self
    }
}

impl ConvertBack for AnnotatedTextEdit {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        AnnotatedTextEdit {
            text_edit: self.text_edit.convert_back(options).await,
            ..self
        }
    }
}

impl ConvertBack for InsertReplaceEdit {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        let insert_range = renderer.get_original_range(uri, &self.insert);
        let replace_range = renderer.get_original_range(uri, &self.replace);
        if let Some(insert_range) = insert_range {
            if let Some(replace_range) = replace_range {
                return InsertReplaceEdit {
                    new_text: self.new_text,
                    insert: insert_range,
                    replace: replace_range,
                };
            }
        }
        self
    }
}

impl ConvertBack for CompletionItem {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        if let Some(text_edit) = self.text_edit {
            match text_edit {
                CompletionTextEdit::Edit(edit) => CompletionItem {
                    text_edit: Some(CompletionTextEdit::Edit(edit.convert_back(options).await)),
                    ..self
                },
                CompletionTextEdit::InsertAndReplace(edit) => CompletionItem {
                    text_edit: Some(CompletionTextEdit::InsertAndReplace(
                        edit.convert_back(options).await,
                    )),
                    ..self
                },
            }
        } else {
            self
        }
    }
}

impl ConvertBack for CompletionList {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let mut result = vec![];
        for item in self.items {
            result.push(item.convert_back(options).await);
        }
        CompletionList {
            items: result,
            is_incomplete: self.is_incomplete,
        }
    }
}

impl ConvertBack for CompletionResponse {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            CompletionResponse::Array(array) => {
                let mut result = vec![];
                for item in array {
                    result.push(item.convert_back(options).await);
                }
                CompletionResponse::Array(result)
            }
            CompletionResponse::List(list) => {
                CompletionResponse::List(list.convert_back(options).await)
            }
        }
    }
}

impl ConvertBack for Location {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        Location {
            uri: self.uri.convert_back(options).await,
            ..self
        }
    }
}

impl ConvertBack for LocationLink {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let target_uri = self.target_uri.convert_back(options).await;
        LocationLink { target_uri, ..self }
    }
}

impl ConvertBack for GotoDefinitionResponse {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            GotoDefinitionResponse::Array(array) => {
                let mut result = vec![];
                for item in array {
                    result.push(item.convert_back(options).await);
                }
                GotoDefinitionResponse::Array(result)
            }
            GotoDefinitionResponse::Link(link) => {
                let mut result = vec![];
                for item in link {
                    result.push(item.convert_back(options).await);
                }
                GotoDefinitionResponse::Link(result)
            }
            GotoDefinitionResponse::Scalar(scalar) => {
                GotoDefinitionResponse::Scalar(scalar.convert_back(options).await)
            }
        }
    }
}

impl ConvertBack for Vec<DocumentSymbol> {
    /// 必须 root_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        fn inner_convert_back(
            symbol_list: Vec<DocumentSymbol>,
            text_document: Option<&FullTextDocument>,
            renderer: &Renderer,
        ) -> Vec<DocumentSymbol> {
            let mut result = vec![];
            for mut item in symbol_list {
                let valid = Renderer::is_position_valid_by_document(
                    text_document,
                    &item.selection_range.start,
                );
                if valid {
                    if let Some(children) = item.children {
                        item.children = Some(inner_convert_back(children, text_document, renderer));
                    }
                    if !Renderer::is_position_valid_by_document(
                        text_document,
                        &item.selection_range.end,
                    ) {
                        let end_character = Renderer::get_line_end_by_document(
                            text_document,
                            item.selection_range.end.line,
                        );
                        item.selection_range.end.character = end_character;
                    }
                    if !Renderer::is_position_valid_by_document(text_document, &item.range.end) {
                        let end_character =
                            Renderer::get_line_end_by_document(text_document, item.range.end.line);
                        item.range.end.character = end_character;
                    }
                    if item.selection_range.start < item.selection_range.end
                        && item.range.start < item.range.end
                    {
                        result.push(item);
                    }
                }
            }
            result
        }

        inner_convert_back(self, renderer.get_document(uri), renderer)
    }
}

impl ConvertBack for Vec<SymbolInformation> {
    /// 必须 root_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let (root_uri, _) = options.root_uri_target_uri();
        let renderer = options.renderer.unwrap();
        let mut result = vec![];
        for mut item in self {
            let valid = renderer.is_position_valid(&item.location.uri, &item.location.range.start);
            if valid {
                if !renderer.is_position_valid(root_uri, &item.location.range.end) {
                    let end_character =
                        renderer.get_line_end(root_uri, item.location.range.end.line);
                    item.location.range.end.character = end_character;
                }
                result.push(item);
            }
        }
        result
    }
}

impl ConvertBack for DocumentSymbolResponse {
    /// 必须 root_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            DocumentSymbolResponse::Nested(symbol) => {
                DocumentSymbolResponse::Nested(symbol.convert_back(options).await)
            }
            DocumentSymbolResponse::Flat(symbol) => {
                DocumentSymbolResponse::Flat(symbol.convert_back(options).await)
            }
        }
    }
}

impl ConvertBack for Vec<SemanticToken> {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        // 从 render_insert_offset 分断，后面的移到前面，重新计算前面部分的位置，以及后面部分第一个 token 的位置
        if let Some(render_insert_offset) = renderer.get_render_insert_offset(uri) {
            let document = renderer.get_document(uri).unwrap();
            let pos = document.position_at(render_insert_offset as u32 + 1);

            // 从 render_insert_offset 分断，分别移到 template 和 script 中
            let render_method_line = pos.line;
            let render_method_character = pos.character - 1;
            let mut template = vec![];
            let mut script = vec![];

            let mut prev_line = 0;
            let mut prev_character = 0;
            // 将 token 分为 template 和 script 并且将相对坐标改为绝对坐标
            for mut token in self {
                token.delta_line += prev_line;

                if token.delta_line == prev_line {
                    token.delta_start += prev_character;
                }
                prev_character = token.delta_start;
                prev_line = token.delta_line;

                if token.delta_line < render_method_line {
                    script.push(token);
                } else if token.delta_line > render_method_line {
                    template.push(token);
                } else if token.delta_start < render_method_character {
                    script.push(token);
                } else {
                    template.push(token);
                }
            }

            let mut result = vec![];
            // 重新计算 template 中的表达式的坐标
            prev_line = 0;
            prev_character = 0;
            for mut token in template {
                if let Some(start) = renderer.get_original_position(
                    uri,
                    &Position {
                        line: token.delta_line,
                        character: token.delta_start,
                    },
                ) {
                    token.delta_line = start.line - prev_line;
                    if token.delta_line > 0 {
                        prev_character = 0;
                    }
                    token.delta_start = start.character - prev_character;
                    result.push(token);

                    prev_line = start.line;
                    prev_character = start.character;
                }
            }
            // 重新计算 script 中的 token 的坐标
            for mut token in script {
                token.delta_line -= prev_line;
                if token.delta_line > 0 {
                    prev_character = 0;
                }
                token.delta_start -= prev_character;
                result.push(token);

                prev_line += token.delta_line;
                prev_character += token.delta_start;
            }
            result
        } else {
            self
        }
    }
}

impl ConvertBack for SemanticTokens {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        SemanticTokens {
            result_id: self.result_id,
            data: self.data.convert_back(options).await,
        }
    }
}

impl ConvertBack for SemanticTokensPartialResult {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        SemanticTokensPartialResult {
            data: self.data.convert_back(options).await,
        }
    }
}

impl ConvertBack for SemanticTokensResult {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            SemanticTokensResult::Tokens(tokens) => {
                SemanticTokensResult::Tokens(tokens.convert_back(options).await)
            }
            SemanticTokensResult::Partial(partial) => {
                SemanticTokensResult::Partial(partial.convert_back(options).await)
            }
        }
    }
}

impl ConvertBack for SemanticTokensRangeResult {
    /// 必须 uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            SemanticTokensRangeResult::Tokens(tokens) => {
                SemanticTokensRangeResult::Tokens(tokens.convert_back(options).await)
            }
            SemanticTokensRangeResult::Partial(tokens) => {
                SemanticTokensRangeResult::Partial(tokens.convert_back(options).await)
            }
        }
    }
}

impl ConvertBack for HashMap<Url, Vec<TextEdit>> {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let mut result = HashMap::new();
        for (url, list) in self {
            result.insert(
                url.convert_back(options).await,
                list.convert_back(options).await,
            );
        }
        result
    }
}

impl ConvertBack for OptionalVersionedTextDocumentIdentifier {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        OptionalVersionedTextDocumentIdentifier {
            uri: self.uri.convert_back(options).await,
            version: self.version,
        }
    }
}

impl ConvertBack for TextDocumentEdit {
    /// 必须 root_uri, target_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let text_document = self.text_document.convert_back(options).await;
        let uri = text_document.uri.clone();
        let options = ConvertOptions {
            uri: Some(&uri),
            ..options.clone()
        };
        TextDocumentEdit {
            text_document,
            edits: self.edits.convert_back(&options).await,
        }
    }
}

impl ConvertBack for CreateFile {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        CreateFile {
            uri: self.uri.convert_back(options).await,
            ..self
        }
    }
}

impl ConvertBack for DeleteFile {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        DeleteFile {
            uri: self.uri.convert_back(options).await,
            ..self
        }
    }
}

impl ConvertBack for RenameFile {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        RenameFile {
            old_uri: self.old_uri.convert_back(options).await,
            new_uri: self.new_uri.convert_back(options).await,
            ..self
        }
    }
}

impl ConvertBack for ResourceOp {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            ResourceOp::Create(create) => ResourceOp::Create(create.convert_back(options).await),
            ResourceOp::Delete(delete) => ResourceOp::Delete(delete.convert_back(options).await),
            ResourceOp::Rename(rename) => ResourceOp::Rename(rename.convert_back(options).await),
        }
    }
}

impl ConvertBack for DocumentChangeOperation {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            DocumentChangeOperation::Edit(edit) => {
                DocumentChangeOperation::Edit(edit.convert_back(options).await)
            }
            DocumentChangeOperation::Op(op) => {
                DocumentChangeOperation::Op(op.convert_back(options).await)
            }
        }
    }
}

impl ConvertBack for DocumentChanges {
    /// 必须 root_uri, target_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        match self {
            DocumentChanges::Edits(edits) => {
                DocumentChanges::Edits(edits.convert_back(options).await)
            }
            DocumentChanges::Operations(o) => {
                DocumentChanges::Operations(o.convert_back(options).await)
            }
        }
    }
}

impl ConvertBack for HashMap<ChangeAnnotationIdentifier, ChangeAnnotation> {
    async fn convert_back(self, _: &ConvertOptions<'_>) -> Self {
        self
    }
}

impl ConvertBack for WorkspaceEdit {
    /// 必须 root_uri, target_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        WorkspaceEdit {
            changes: self.changes.convert_back(options).await,
            document_changes: self.document_changes.convert_back(options).await,
            change_annotations: self.change_annotations.convert_back(options).await,
        }
    }
}

impl ConvertBack for DiagnosticRelatedInformation {
    /// 必须 root_uri, target_uri
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        DiagnosticRelatedInformation {
            location: self.location.convert_back(options).await,
            message: self.message,
        }
    }
}

impl ConvertBack for Vec<Diagnostic> {
    /// 必须 uri, root_uri, target_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let uri = options.uri.unwrap();
        let renderer = options.renderer.unwrap();
        let mut diags = vec![];
        for mut diag in self {
            let start = renderer.get_original_position(&uri, &diag.range.start);
            if let Some(start) = start {
                let end = Position {
                    line: start.line,
                    character: start.character + diag.range.end.character
                        - diag.range.start.character,
                };
                diag.range = Range { start, end };
                diag.related_information = diag.related_information.convert_back(options).await;
                diags.push(diag);
            } else if renderer.is_position_valid(&uri, &diag.range.start) {
                diag.related_information = diag.related_information.convert_back(options).await;
                diags.push(diag);
            }
        }
        diags
    }
}

impl ConvertBack for Command {
    /// 只需要默认值
    async fn convert_back(self, _: &ConvertOptions<'_>) -> Self {
        Command {
            command: self
                .command
                .replace("_typescript", "vue2-ts-decorator_typescript"),
            ..self
        }
    }
}

impl ConvertBack for CodeAction {
    /// 只需要默认值
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        if let Some(command) = self.command {
            CodeAction {
                command: Some(command.convert_back(options).await),
                ..self
            }
        } else {
            self
        }
    }
}

impl ConvertBack for CodeActionResponse {
    /// 只需要默认值
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        let mut code_actions = vec![];
        for item in self {
            code_actions.push(match item {
                CodeActionOrCommand::CodeAction(code_action) => {
                    CodeActionOrCommand::CodeAction(code_action.convert_back(options).await)
                }
                CodeActionOrCommand::Command(command) => {
                    CodeActionOrCommand::Command(command.convert_back(options).await)
                }
            });
        }
        code_actions
    }
}

impl ConvertBack for ApplyWorkspaceEditParams {
    /// 必须 root_uri, target_uri, renderer
    async fn convert_back(self, options: &ConvertOptions<'_>) -> Self {
        ApplyWorkspaceEditParams {
            label: self.label,
            edit: self.edit.convert_back(options).await,
        }
    }
}
