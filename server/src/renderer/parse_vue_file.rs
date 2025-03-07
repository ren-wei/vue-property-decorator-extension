use lsp_textdocument::FullTextDocument;

use super::{
    parse_document::{self, ExtendsComponent},
    render_tree::{InitRenderCache, VueResolvingCache},
    template_compile,
};

/// 初始化时初步解析 vue 文件
pub fn init_parse_vue_file(
    document: &FullTextDocument,
) -> Option<(InitRenderCache, Option<ExtendsComponent>)> {
    // 解析文档
    let (template, script, _) = parse_document::parse_document(&document);

    let template = template?;
    let script = script?;
    let source = document.get_content(None);
    // 解析脚本
    let (props, render_insert_offset, extends_component, _) =
        parse_document::parse_script(&script, source)?;
    // 模版编译
    let (template_compile_result, _) = template_compile::template_compile(&template, source);

    Some((
        InitRenderCache::VueResolving(VueResolvingCache {
            script_start_pos: script.start_tag_end.unwrap(),
            script_end_pos: script.end_tag_start.unwrap(),
            template_compile_result,
            props,
            render_insert_offset,
            source: source.to_string(),
        }),
        extends_component,
    ))
}
