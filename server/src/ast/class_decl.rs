use html_languageservice::html_data::Description;
use swc_common::{comments::Comments, BytePos};
use swc_ecma_ast::ClassDecl;
use tower_lsp::lsp_types::{MarkupContent, MarkupKind};

use crate::renderer::multi_threaded_comment::MultiThreadedComments;

use super::comment::get_markdown;

pub fn get_class_decl_pos(class: &ClassDecl) -> BytePos {
    let class = &class.class;
    let mut pos = class.span.lo;
    let decorators = &class.decorators;
    if decorators.len() > 0 {
        pos = decorators[0].span.lo;
    }
    pos
}

pub fn get_class_decl_name(class: &ClassDecl) -> String {
    class.ident.sym.to_string()
}

pub fn get_class_decl_description(
    class: &ClassDecl,
    comments: &MultiThreadedComments,
) -> Option<Description> {
    let comments = comments.get_leading(get_class_decl_pos(class));
    let value = if let Some(comments) = comments {
        comments
            .iter()
            .map(get_markdown)
            .collect::<Vec<String>>()
            .join("\n")
    } else {
        "".to_string()
    };
    Some(Description::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: format!(
            "```typescript\nclass {}\n```\n{}",
            get_class_decl_name(class),
            value
        ),
    }))
}
