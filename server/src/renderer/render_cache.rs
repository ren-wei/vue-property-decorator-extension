use html_languageservice::parser::html_document::Node;
use lsp_textdocument::FullTextDocument;
use tower_lsp::lsp_types::Url;

use super::template_compile::CompileMapping;

pub enum RenderCache {
    VueRenderCache(VueRenderCache),
    Unknown,
}

/// vue 组件的渲染缓存
pub struct VueRenderCache {
    /// 渲染前的文档，与文件系统中相同
    pub document: FullTextDocument,
    pub template: Node,
    pub script: Node,
    pub style: Vec<Node>,
    /// 渲染得到的属性
    pub props: Vec<String>,
    pub render_insert_offset: usize,
    pub template_compile_result: String,
    pub mapping: CompileMapping,
    pub extends_component: FinalExtendsComponent,
}

pub struct FinalExtendsComponent {
    pub uri: Url,
    pub export_name: Option<String>,
}
