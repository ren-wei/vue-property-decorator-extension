use html_languageservice::{parser::html_document::Node, HTMLDataManager};
use lsp_textdocument::FullTextDocument;
use tower_lsp::lsp_types::Range;

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
        if root.tag.as_ref().is_some_and(|v| v == "script") {
            if root.start_tag_end.is_some() && root.end_tag_start.is_some() {
                script = Some(root);
            }
        } else if root.tag.as_ref().is_some_and(|v| v == "template") {
            template = Some(root);
        } else if root.tag.as_ref().is_some_and(|v| v == "style") {
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
