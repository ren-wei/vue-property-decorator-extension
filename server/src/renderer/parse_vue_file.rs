use html_languageservice::parser::html_document::Node;
use lsp_textdocument::FullTextDocument;

use super::{
    parse_document::{self},
    parse_script::{self, ExtendsComponent, ParseScriptResult, RegisterComponent},
    template_compile::{self, CompileMapping},
};

/// 解析 vue 组件
pub fn parse_vue_file(document: &FullTextDocument) -> Option<ParseVueFileResult> {
    // 解析文档
    let (template, script, style) = parse_document::parse_document(&document);

    let template = template?;
    let script = script?;
    let source = document.get_content(None);
    // 解析脚本
    let ParseScriptResult {
        props,
        render_insert_offset,
        extends_component,
        registers,
    } = parse_script::parse_script(
        source,
        script.start_tag_end.unwrap(),
        script.end_tag_start.unwrap(),
    )?;
    // 模版编译
    let (template_compile_result, mapping) = template_compile::template_compile(&template, source);

    Some(ParseVueFileResult {
        template,
        script,
        style,
        props,
        render_insert_offset,
        template_compile_result,
        mapping,
        extends_component,
        registers,
    })
}

pub struct ParseVueFileResult {
    pub template: Node,
    pub script: Node,
    pub style: Vec<Node>,
    /// 渲染得到的属性
    pub props: Vec<String>,
    pub render_insert_offset: usize,
    pub template_compile_result: String,
    pub mapping: CompileMapping,
    pub extends_component: Option<ExtendsComponent>,
    pub registers: Vec<RegisterComponent>,
}
