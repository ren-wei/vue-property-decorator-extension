use lsp_textdocument::FullTextDocument;
use tower_lsp::lsp_types::Url;
use tracing::error;

use crate::ast::{self, TsFileExportResult};

use super::{
    parse_script::{self, ExtendsComponent, RegisterComponent},
    Renderer,
};

/// # 解析 ts 文件
/// 如果 ts 文件默认导出组件，那么进行解析
/// 如果不存在导入导出组件，那么返回 None
pub fn parse_ts_file(document: &FullTextDocument) -> Option<ParseTsFileResult> {
    let source = document.get_content(None);
    let (module, _) = ast::parse_source(source, 0, source.len());
    if let Err(e) = module {
        error!("parse_ts_file error: {:?}", e);
        return None;
    }
    let module = module.unwrap();
    let mut ts_component = None;
    if let Some((props, _, extends_component, registers)) = parse_script::parse_module(&module) {
        ts_component = Some((props, extends_component, registers));
    }
    let (local_exports, transfers) = ast::get_local_exports_and_transfers(&module);
    Some(ParseTsFileResult {
        ts_component,
        local_exports,
        transfers,
    })
}

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

pub struct ParseTsFileResult {
    pub ts_component: Option<(
        Vec<String>,
        Option<ExtendsComponent>,
        Vec<RegisterComponent>,
    )>,
    /// 从当前文件定义的导出
    pub local_exports: Vec<Option<String>>,
    /// 从当前文件引入并导出的所有值 Vec<(local, export_name, path, is_star_export)>
    pub transfers: Vec<(Option<String>, Option<String>, String, bool)>,
}
