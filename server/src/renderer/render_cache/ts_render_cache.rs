use html_languageservice::html_data::Description;
use lsp_textdocument::FullTextDocument;
use swc_common::source_map::SmallPos;
use tower_lsp::lsp_types::Range;
use tower_lsp::lsp_types::TextDocumentContentChangeEvent;
use tower_lsp::lsp_types::Url;
use tracing::error;

use crate::ast::{self, TsFileExportResult};
use crate::renderer::parse_script;
use crate::renderer::parse_script::ExtendsComponent;
use crate::renderer::parse_script::ParseScriptResult;
use crate::renderer::parse_script::RegisterComponent;

use super::RenderCacheUpdateResult;
use super::Renderer;

/// ts 文件的渲染缓存
pub struct TsRenderCache {
    /// ts 文件中定义的组件
    pub ts_component: Option<TsComponent>,
    /// 从当前文件定义并导出的名称
    pub local_exports: Vec<Option<String>>,
}

pub struct TsComponent {
    pub name_range: Range,
    pub description: Option<Description>,
    pub props: Vec<String>,
}

impl TsRenderCache {
    pub fn update(
        &mut self,
        change: TextDocumentContentChangeEvent,
        document: &FullTextDocument,
    ) -> Option<RenderCacheUpdateResult> {
        let result = parse_ts_file(document);
        self.local_exports = result.local_exports;
        if let Some(ts_component) = result.ts_component {
            self.ts_component = Some(TsComponent {
                name_range: ts_component.0,
                description: ts_component.1,
                props: ts_component.2,
            });
            Some(RenderCacheUpdateResult {
                changes: vec![change],
                is_change_prop: true,
                extends_component: ts_component.3,
                registers: Some(ts_component.4),
                transfers: Some(result.transfers),
            })
        } else {
            let is_change_prop = self.ts_component.is_some();
            self.ts_component = None;
            Some(RenderCacheUpdateResult {
                changes: vec![change],
                is_change_prop,
                extends_component: None,
                registers: None,
                transfers: Some(result.transfers),
            })
        }
    }
}

/// # 解析 ts 文件
/// 如果 ts 文件默认导出组件，那么进行解析
/// 如果不存在导入导出组件，那么返回 None
pub fn parse_ts_file(document: &FullTextDocument) -> ParseTsFileResult {
    let source = document.get_content(None);
    let (module, comments) = ast::parse_source(source, 0, source.len());
    if let Err(e) = module {
        error!("parse_ts_file error: {:?}", e);
        return ParseTsFileResult {
            ts_component: None,
            local_exports: vec![],
            transfers: vec![],
        };
    }
    let module = module.unwrap();
    let mut ts_component = None;
    if let Some(ParseScriptResult {
        name_span,
        description,
        props,
        extends_component,
        registers,
        render_insert_offset: _,
        safe_update_range: _,
    }) = parse_script::parse_module(&module, &comments)
    {
        let name_range = Range::new(
            document.position_at(name_span.lo.to_u32()),
            document.position_at(name_span.hi.to_u32()),
        );
        ts_component = Some((name_range, description, props, extends_component, registers));
    }
    let (local_exports, transfers) = ast::get_local_exports_and_transfers(&module);
    ParseTsFileResult {
        ts_component,
        local_exports,
        transfers,
    }
}

/// # 从 ts 文件获取指定导出项
/// * 如果是从当前文件定义，那么返回 None
/// * 如果是从其他文件导出，那么返回新的 path 和导出名称
/// * 如果未找到指定的导出，并且存在所有导出，那么返回所有导出的列表
/// * 如果未找到指定的导出，那么返回 None
pub async fn _parse_ts_file_export(uri: &Url, export_name: &Option<String>) -> TsFileExportResult {
    let document = Renderer::get_document_from_file(uri).await;
    if let Err(e) = document {
        error!("parse_ts_file_export error {}: {}", uri.as_str(), e);
        return TsFileExportResult::_None;
    }
    let document = document.unwrap();
    let source = document.get_content(None);
    let (module, _) = ast::parse_source(source, 0, source.len());
    if let Err(e) = module {
        error!("parse_ts_file_export error {}: {:?}", uri.as_str(), e);
        return TsFileExportResult::_None;
    }
    ast::_get_export_from_module(&module.unwrap(), export_name)
}

pub struct ParseTsFileResult {
    pub ts_component: Option<(
        Range,
        Option<Description>,
        Vec<String>,
        Option<ExtendsComponent>,
        Vec<RegisterComponent>,
    )>,
    /// 从当前文件定义的导出
    pub local_exports: Vec<Option<String>>,
    /// 从当前文件引入并导出的所有值 Vec<(local, export_name, path, is_star_export)>
    pub transfers: Vec<(Option<String>, Option<String>, String, bool)>,
}
