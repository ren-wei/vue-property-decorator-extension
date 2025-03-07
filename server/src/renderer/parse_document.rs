use html_languageservice::{parser::html_document::Node, HTMLDataManager};
use lsp_textdocument::FullTextDocument;
use swc_common::source_map::SmallPos;
use tower_lsp::lsp_types::Range;
use tracing::debug;

use crate::ast;

/// 解析文档，输出 template 节点和 script 节点，并确保 script 节点存在 start_tag_end 和 end_tag_start
pub fn parse_document(document: &FullTextDocument) -> (Option<Node>, Option<Node>, Vec<Node>) {
    let empty_data_manager = HTMLDataManager::new(false, None);
    let html_document = html_languageservice::parse_html_document(
        document.get_content(None),
        document.language_id(),
        &empty_data_manager,
    );
    let mut script = None;
    let mut template = None;
    let mut style = vec![];
    for root in html_document.roots {
        if root.tag == Some("script".to_string()) {
            if root.start_tag_end.is_some() && root.end_tag_start.is_some() {
                script = Some(root);
            }
        } else if root.tag == Some("template".to_string()) {
            template = Some(root);
        } else if root.tag == Some("style".to_string()) {
            style.push(root);
        }
    }
    (template, script, style)
}

/// 将文档指定范围解析为节点
pub fn parse_as_node(document: &FullTextDocument, range: Option<Range>) -> Option<Node> {
    let empty_data_manager = HTMLDataManager::new(false, None);
    let html_document = html_languageservice::parse_html_document(
        document.get_content(range),
        document.language_id(),
        &empty_data_manager,
    );
    if html_document.roots.len() == 1 {
        Some(html_document.roots[0].clone())
    } else {
        None
    }
}

/// 解析脚本，输出 props, render_insert_offset, extends_component, registers
pub fn parse_script(
    script: &Node,
    source: &str,
) -> Option<(
    Vec<String>,
    usize,
    Option<ExtendsComponent>,
    Vec<RegisterComponent>,
)> {
    let start_tag_end = if let Some(start_tag_end) = script.start_tag_end {
        start_tag_end
    } else {
        script.start
    };
    let end_tag_start = if let Some(end_tag_start) = script.end_tag_start {
        end_tag_start
    } else {
        script.end
    };
    let (module, _) = ast::parse_source(source, start_tag_end, end_tag_start);
    if let Ok(module) = &module {
        let mut extends_component = None;
        let mut registers = vec![];
        if let Some(class) = ast::get_default_class_expr_from_module(module) {
            let mut props = vec![];
            for member in class
                .class
                .body
                .iter()
                .filter(|v| ast::filter_all_prop_method(v))
                .collect::<Vec<_>>()
            {
                props.push(ast::get_class_member_name(member));
            }
            let extends_ident = ast::get_extends_component(class);
            if let Some(extends_ident) = extends_ident {
                if let Some((orig_name, path)) = ast::get_import_from_module(module, &extends_ident)
                {
                    if !orig_name.as_ref().is_some_and(|v| v == "Vue") {
                        extends_component = Some(ExtendsComponent {
                            name: orig_name,
                            path,
                        });
                    }
                }
            }
            let render_insert_offset = class.class.span.hi.to_usize() - 1;
            return Some((props, render_insert_offset, extends_component, registers));
        }
    }
    None
}

/// 继承的组件
#[derive(Debug)]
pub struct ExtendsComponent {
    /// 导出的组件名，如果是默认导出，则为 None，如果被重命名，那么则为重命名前的名称
    pub name: Option<String>,
    /// 导入路径
    pub path: String,
}

/// 注册的组件
pub struct RegisterComponent {
    /// 注册的名称
    pub name: String,
    /// 导出的名称
    pub export: Option<String>,
    /// 导入路径
    pub path: String,
}
