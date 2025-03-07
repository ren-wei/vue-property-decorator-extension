use lsp_textdocument::FullTextDocument;
use tower_lsp::lsp_types::Url;
use tracing::error;

use crate::ast::{self, TsFileExportResult};

use super::{
    parse_document::ExtendsComponent,
    render_tree::{InitRenderCache, TsResolvedCache},
    Renderer,
};

/// # 从 ts 文件获取指定导出项
/// * 如果是从当前文件定义，那么返回 None
/// * 如果是从其他文件导出，那么返回新的 path 和导出名称
/// * 如果未找到指定的导出，并且存在所有导出，那么返回所有导出的列表
/// * 如果未找到指定的导出，那么返回 None
pub async fn parse_ts_file_export(uri: &Url, export_name: &Option<String>) -> TsFileExportResult {
    let document = Renderer::get_document_from_file(uri).await;
    if let Err(e) = document {
        error!("parse_ts_file_export error {}: {}", uri.as_str(), e);
        return TsFileExportResult::None;
    }
    let document = document.unwrap();
    let source = document.get_content(None);
    let (module, _) = ast::parse_source(source, 0, source.len());
    if let Err(e) = module {
        error!("parse_ts_file_export error {}: {:?}", uri.as_str(), e);
        return TsFileExportResult::None;
    }
    ast::get_export_from_module(&module.unwrap(), export_name)
}

/// # 解析 ts 文件
/// 如果 ts 文件默认导出组件，那么进行解析
/// 如果不存在导入导出组件，那么返回 None
pub async fn parse_ts_file(
    document: &FullTextDocument,
) -> Option<(InitRenderCache, Option<ExtendsComponent>)> {
    let source = document.get_content(None);
    let (module, _) = ast::parse_source(source, 0, source.len());
    if let Err(e) = module {
        error!("parse_ts_file error: {:?}", e);
        return None;
    }
    let module = module.unwrap();
    let class = ast::get_default_class_expr_from_module(&module)?;
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
    let mut extends_component = None;
    let extends_ident = ast::get_extends_component(class);
    if let Some(extends_ident) = extends_ident {
        if let Some((orig_name, path)) = ast::get_import_from_module(&module, &extends_ident) {
            extends_component = Some(ExtendsComponent {
                name: orig_name,
                path,
            });
        }
    }
    Some((
        InitRenderCache::TsResolved(TsResolvedCache { props }),
        extends_component,
    ))
}
