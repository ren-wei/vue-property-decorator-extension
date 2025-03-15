mod combined_rendered_results;
mod mapping;
pub mod multi_threaded_comment;
mod parse_document;
mod parse_import_path;
mod parse_lib;
mod parse_script;
mod parse_ts_file;
mod parse_vue_file;
mod render;
mod render_cache;
mod tags_provider;
mod template_compile;

use html_languageservice::parser::html_document::HTMLDocument;
pub use mapping::Mapping;
pub use mapping::PositionType;
pub use render::Render;
use render_cache::RenderCache;
use render_cache::RenderCacheGraph;
use tags_provider::ArcTagsProvider;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tower_lsp::lsp_types::Location;

use std::{collections::HashMap, env::consts::OS, io::Error, path::PathBuf};

use lsp_textdocument::FullTextDocument;
use tower_lsp::lsp_types::{Position, Range, Url};
use tracing::error;

/// # 渲染器
/// 将项目渲染到同目录下的加上 `.~$` 前缀的目录中
pub struct Renderer {
    root_uri_target_uri: Option<(Url, Url)>,
    alias: HashMap<String, String>,
    render_cache: RenderCacheGraph,
    provider_map: HashMap<Url, ArcTagsProvider>,
    /// 组件库列表
    library_list: Vec<Url>,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            root_uri_target_uri: None,
            alias: HashMap::new(),
            render_cache: RenderCacheGraph::new(),
            provider_map: HashMap::new(),
            library_list: vec![],
        }
    }

    pub fn root_uri_target_uri(&self) -> &Option<(Url, Url)> {
        &self.root_uri_target_uri
    }

    pub fn get_document(&self, uri: &Url) -> Option<&FullTextDocument> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(&cache.document)
        } else {
            None
        }
    }

    pub fn get_html_document(&self, uri: &Url) -> Option<HTMLDocument> {
        let cache = &self.render_cache[uri];
        if let RenderCache::VueRenderCache(cache) = cache {
            let mut roots = vec![cache.template.clone(), cache.script.clone()];
            roots.append(&mut cache.style.clone());
            Some(HTMLDocument { roots })
        } else {
            None
        }
    }

    pub fn get_render_insert_offset(&self, uri: &Url) -> Option<usize> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(cache.render_insert_offset)
        } else {
            None
        }
    }

    /// 获取标签对应的组件位置
    pub fn get_component_location(&self, uri: &Url, tag: &str) -> Option<Location> {
        let (registered_uri, register) = self.render_cache.get_register(uri, tag)?;
        let node = self.render_cache.get(registered_uri)?;
        let range = match node {
            RenderCache::VueRenderCache(cache) => cache.name_range,
            RenderCache::TsRenderCache(cache) => {
                if register.export_name.is_none() {
                    cache.ts_component.as_ref()?.name_range
                } else {
                    // TODO: 根据 register.export_name 获取实际组件
                    Range::default()
                }
            }
            RenderCache::LibRenderCache(cache) => {
                let component = cache.components.iter().find(|c| {
                    register
                        .export_name
                        .as_ref()
                        .is_some_and(|name| name == &c.name)
                })?;
                return Some(component.name_location.clone());
            }
            RenderCache::Unknown => Range::default(),
        };
        Some(Location {
            uri: registered_uri.clone(),
            range,
        })
    }
}

/// tools
impl Renderer {
    pub fn get_line_end(&self, uri: &Url, line: u32) -> u32 {
        Renderer::get_line_end_by_document(self.get_document(uri), line)
    }

    /// 脚本开始位置
    pub fn start_position(&self, uri: &Url) -> Option<Position> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(
                cache
                    .document
                    .position_at(cache.script.start_tag_end.unwrap() as u32),
            )
        } else {
            None
        }
    }

    /// 脚本结束位置
    pub fn end_position(&self, uri: &Url) -> Option<Position> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(
                cache
                    .document
                    .position_at(cache.script.end_tag_start.unwrap() as u32),
            )
        } else {
            None
        }
    }
    pub async fn get_document_from_file(uri: &Url) -> Result<FullTextDocument, Error> {
        let mut content = String::new();
        let temp_path;

        let path: &str = if OS == "windows" {
            temp_path =
                percent_encoding::percent_decode(&uri.path()[1..].as_bytes()).decode_utf8_lossy();
            &temp_path
        } else {
            temp_path =
                percent_encoding::percent_decode(&uri.path().as_bytes()).decode_utf8_lossy();
            &temp_path
        };
        match File::open(path).await {
            Ok(mut file) => {
                if let Err(err) = file.read_to_string(&mut content).await {
                    error!("error: {} - {}", path, err);
                    return Err(err);
                }
            }
            Err(err) => {
                error!("error: {} - {}", path, err);
                return Err(Error::new(std::io::ErrorKind::NotFound, path));
            }
        }
        let language_id = uri.path()[uri.path().rfind(".").unwrap() + 1..].to_string();
        Ok(FullTextDocument::new(language_id, 1, content))
    }

    pub fn is_vue_component(uri: &Url) -> bool {
        uri.to_file_path()
            .unwrap()
            .extension()
            .is_some_and(|v| v == "vue")
    }

    /// uri 是否有效
    /// * 是文件
    /// * 存在于文件系统中
    /// * 不在 node_modules 中
    pub fn is_uri_valid(uri: &Url) -> bool {
        let file_path = uri.to_file_path();
        if let Ok(file_path) = file_path {
            file_path.exists()
                && file_path.is_file()
                && !file_path.to_string_lossy().contains("/node_modules/")
        } else {
            false
        }
    }

    /// uri 是否指向 node_modules 下的库
    /// * 是目录
    /// * 存在于文件系统中
    pub fn is_node_modules(uri: &Url) -> bool {
        let file_path = uri.to_file_path();
        if let Ok(file_path) = file_path {
            file_path.exists()
                && file_path.is_dir()
                && file_path.to_string_lossy().contains("/node_modules/")
        } else {
            false
        }
    }

    pub fn is_position_valid_by_document(
        document: Option<&FullTextDocument>,
        position: &Position,
    ) -> bool {
        if let Some(document) = document {
            let start = document.offset_at(Position::new(position.line, 0));
            let end = document.offset_at(Position::new(position.line + 1, 0));
            position.character < end - start
        } else {
            false
        }
    }

    pub fn get_line_end_by_document(document: Option<&FullTextDocument>, line: u32) -> u32 {
        if let Some(document) = document {
            let start = document.offset_at(Position::new(line, 0));
            let end = document.offset_at(Position::new(line, u32::MAX));
            let content = document.get_content(Some(Range::new(
                document.position_at(start),
                document.position_at(end),
            )));
            if content.ends_with("\r\n") {
                end - start - 2
            } else if content.ends_with("\n") {
                end - start - 1
            } else {
                end - start
            }
        } else {
            0
        }
    }

    /// 获取目标路径
    fn get_target_path(uri: &Url, root_uri: &Url, target_root_uri: &Url) -> PathBuf {
        let src_path = uri.to_file_path().unwrap();
        let root_path = root_uri.to_file_path().unwrap();
        let target_root_path = target_root_uri.to_file_path().unwrap();
        // 计算相对路径
        let rel_path = src_path.strip_prefix(&root_path).unwrap().to_path_buf();
        // 转换为目标路径
        let mut target_path = target_root_path.join(rel_path);
        if let Some(file_name) = target_path.file_name() {
            if target_path.extension().is_some_and(|v| v == "vue") {
                let new_file_name = format!("{}.ts", file_name.to_string_lossy());
                target_path.set_file_name(new_file_name);
            }
        }
        target_path
    }
}
