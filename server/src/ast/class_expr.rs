use html_languageservice::html_data::Description;
use swc_common::{comments::Comments, BytePos};
use swc_ecma_ast::{ClassExpr, Expr};
use tower_lsp::lsp_types::{MarkupContent, MarkupKind};

use crate::renderer::multi_threaded_comment::MultiThreadedComments;

use super::comment::get_markdown;

pub fn get_class_expr_pos(class: &ClassExpr) -> BytePos {
    let class = &class.class;
    let mut pos = class.span.lo;
    let decorators = &class.decorators;
    if decorators.len() > 0 {
        pos = decorators[0].span.lo;
    }
    pos
}

pub fn get_class_expr_name(class: &ClassExpr) -> String {
    if let Some(ident) = &class.ident {
        return ident.sym.to_string();
    }
    "unknown".to_string()
}

pub fn get_class_expr_description(
    class: &ClassExpr,
    comments: &MultiThreadedComments,
) -> Option<Description> {
    let class_expr = class;

    let comments = comments.get_leading(get_class_expr_pos(class_expr));
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
            get_class_expr_name(class),
            value
        ),
    }))
}

pub fn get_extends_component(class: &ClassExpr) -> Option<String> {
    let supper_class = class.class.super_class.as_ref()?;
    if let Expr::Ident(ident) = supper_class.as_ref() {
        Some(ident.sym.to_string())
    } else {
        None
    }
}
