mod combined_rendered_results;
mod mapping;
pub mod multi_threaded_comment;
mod parse_document;
mod parse_import_path;
mod parse_script;
mod render;
mod render_cache;
mod tags_provider;
mod template_compile;

use html_languageservice::parser::html_document::HTMLDocument;
pub use mapping::PositionType;
use render_cache::RenderCache;
use render_cache::RenderCacheGraph;
pub use render_cache::RenderCachePropType;
use tags_provider::ArcTagsProvider;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tower_lsp::lsp_types::Location;

use std::collections::HashSet;
use std::{collections::HashMap, env::consts::OS, io::Error, path::PathBuf};

use lsp_textdocument::FullTextDocument;
use tower_lsp::lsp_types::{Position, Range, Uri};
use tracing::error;

use crate::util;

/// # 渲染器
/// 将项目渲染到同目录下的加上 `.~$` 前缀的目录中
pub struct Renderer {
    root_uri_target_uri: Option<(Uri, Uri)>,
    alias: HashMap<String, String>,
    render_cache: RenderCacheGraph,
    provider_map: HashMap<Uri, ArcTagsProvider>,
    /// 组件库列表
    library_list: Vec<Uri>,
    /// 文件被创建时，将会创建的文件，创建完成后清空
    will_create_files: HashSet<Uri>,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            root_uri_target_uri: None,
            alias: HashMap::new(),
            render_cache: RenderCacheGraph::new(),
            provider_map: HashMap::new(),
            library_list: vec![],
            will_create_files: HashSet::new(),
        }
    }

    pub fn root_uri_target_uri(&self) -> &Option<(Uri, Uri)> {
        &self.root_uri_target_uri
    }

    #[cfg(test)]
    pub fn set_root_uri_target_uri(&mut self, root_uri: Uri, target_uri: Uri) {
        self.root_uri_target_uri = Some((root_uri, target_uri));
    }

    pub fn get_document(&self, uri: &Uri) -> Option<&FullTextDocument> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(&cache.document)
        } else {
            None
        }
    }

    pub fn get_html_document(&self, uri: &Uri) -> Option<HTMLDocument> {
        let cache = &self.render_cache[uri];
        if let RenderCache::VueRenderCache(cache) = cache {
            let mut roots = vec![];
            if let Some(template) = &cache.template {
                roots.push(template.clone());
            }
            if let Some(script) = &cache.script {
                roots.push(script.clone());
            }
            roots.append(&mut cache.style.clone());
            Some(HTMLDocument { roots })
        } else {
            None
        }
    }

    pub fn get_render_insert_offset(&self, uri: &Uri) -> Option<usize> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(cache.render_insert_offset)
        } else {
            None
        }
    }

    pub fn get_component_name(&self, uri: &Uri) -> Option<&str> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(cache.document.get_content(Some(Range::new(
                cache.document.position_at(cache.name_range.0 as u32),
                cache.document.position_at(cache.name_range.1 as u32),
            ))))
        } else {
            None
        }
    }

    /// 获取标签对应的组件位置
    pub fn get_component_location(&self, uri: &Uri, tag: &str) -> Option<Location> {
        let (mut registered_uri, register) = self.render_cache.get_register(uri, tag)?;
        let mut export_name = register.export_name.clone();
        let range;
        loop {
            let node = self.render_cache.get(registered_uri)?;
            match node {
                RenderCache::VueRenderCache(cache) => {
                    range = Range {
                        start: cache.document.position_at(cache.name_range.0 as u32),
                        end: cache.document.position_at(cache.name_range.1 as u32),
                    };
                    break;
                }
                RenderCache::TsRenderCache(cache) => {
                    if export_name.is_none() && cache.ts_component.is_some() {
                        range = cache.ts_component.as_ref()?.name_range;
                        break;
                    } else {
                        let (transfer_uri, export) = self
                            .render_cache
                            .get_transfer_node(registered_uri, &export_name)?;
                        registered_uri = transfer_uri;
                        export_name = export;
                    }
                }
                RenderCache::LibRenderCache(cache) => {
                    let component = cache
                        .components
                        .iter()
                        .find(|c| export_name.as_ref().is_some_and(|name| name == &c.name))?;
                    return Some(component.name_location.clone());
                }
            };
        }
        Some(Location {
            uri: registered_uri.clone(),
            range,
        })
    }

    pub fn get_component_prop_location(
        &self,
        uri: &Uri,
        tag: &str,
        attr: &str,
    ) -> Option<Location> {
        let attr = if attr.starts_with(":") {
            &attr[1..]
        } else {
            attr
        };
        let (mut registered_uri, register) = self.render_cache.get_register(uri, tag)?;
        let mut export_name = register.export_name.clone();
        let range;
        loop {
            let node = self.render_cache.get(registered_uri)?;
            match node {
                RenderCache::VueRenderCache(cache) => {
                    let prop = cache.props.iter().find(|v| v.name == attr)?;
                    range = Range {
                        start: cache.document.position_at(prop.range.0 as u32),
                        end: cache.document.position_at(prop.range.1 as u32),
                    };
                    break;
                }
                RenderCache::TsRenderCache(cache) => {
                    if export_name.is_none() && cache.ts_component.is_some() {
                        let prop = cache
                            .ts_component
                            .as_ref()?
                            .props
                            .iter()
                            .find(|v| v.name == attr)?;
                        range = Range {
                            start: cache.document.position_at(prop.range.0 as u32),
                            end: cache.document.position_at(prop.range.1 as u32),
                        };
                        break;
                    } else {
                        let (transfer_uri, export) = self
                            .render_cache
                            .get_transfer_node(registered_uri, &export_name)?;
                        registered_uri = transfer_uri;
                        export_name = export;
                    }
                }
                RenderCache::LibRenderCache(cache) => {
                    let component = cache
                        .components
                        .iter()
                        .find(|c| export_name.as_ref().is_some_and(|name| name == &c.name))?;
                    let prop = component.props.iter().find(|v| v.name == attr)?;
                    return Some(prop.location.clone());
                }
            };
        }
        Some(Location {
            uri: registered_uri.clone(),
            range,
        })
    }

    pub fn get_component_prop_type(&self, uri: &Uri, prop: &str) -> Option<&str> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            let prop = cache.props.iter().find(|v| v.name == prop)?;
            match prop.typ {
                RenderCachePropType::Property => Some("property"),
                RenderCachePropType::Method => Some("method"),
            }
        } else {
            None
        }
    }
}

/// tools
impl Renderer {
    pub fn get_line_end(&self, uri: &Uri, line: u32) -> u32 {
        Renderer::get_line_end_by_document(self.get_document(uri), line)
    }

    /// 脚本开始位置
    pub fn start_position(&self, uri: &Uri) -> Option<Position> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(
                cache
                    .document
                    .position_at(cache.script.as_ref()?.start_tag_end.unwrap() as u32),
            )
        } else {
            None
        }
    }

    /// 脚本结束位置
    pub fn end_position(&self, uri: &Uri) -> Option<Position> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(
                cache
                    .document
                    .position_at(cache.script.as_ref()?.end_tag_start.unwrap() as u32),
            )
        } else {
            None
        }
    }
    pub async fn get_document_from_file(uri: &Uri) -> Result<FullTextDocument, Error> {
        let mut content = String::new();
        let temp_path;

        let path_str = util::to_file_path_string(uri);
        let path: &str = if OS == "windows" {
            temp_path =
                percent_encoding::percent_decode(&path_str[1..].as_bytes()).decode_utf8_lossy();
            &temp_path
        } else {
            temp_path = percent_encoding::percent_decode(&path_str.as_bytes()).decode_utf8_lossy();
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
        let language_id = path_str[path_str.rfind(".").unwrap() + 1..].to_string();
        Ok(FullTextDocument::new(language_id, 1, content))
    }

    pub fn is_vue_component(uri: &Uri) -> bool {
        util::to_file_path(uri)
            .extension()
            .is_some_and(|v| v == "vue")
    }

    /// uri 是否有效
    /// * 是文件
    /// * 存在于文件系统中
    /// * 不在 node_modules 中
    pub fn is_uri_valid(uri: &Uri) -> bool {
        let file_path = util::to_file_path(uri);
        file_path.exists()
            && file_path.is_file()
            && !file_path.to_string_lossy().contains("/node_modules/")
    }

    /// uri 是否指向 node_modules 下的库
    /// * 是目录
    /// * 存在于文件系统中
    pub fn is_node_modules(uri: &Uri) -> bool {
        let file_path = util::to_file_path(uri);
        file_path.exists()
            && file_path.is_dir()
            && file_path.to_string_lossy().contains("/node_modules/")
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
    fn get_target_path(uri: &Uri, root_uri: &Uri, target_root_uri: &Uri) -> PathBuf {
        let src_path = util::to_file_path(uri);
        let root_path = util::to_file_path(root_uri);
        let target_root_path = util::to_file_path(target_root_uri);
        // 计算相对路径
        let rel_path = src_path.strip_prefix(&root_path).unwrap().to_path_buf();
        // 转换为目标路径
        let mut target_path = target_root_path.join(rel_path);
        if let Some(file_name) = target_path.file_name() {
            if file_name.to_string_lossy().ends_with(".vue") {
                let new_file_name = format!("{}.ts", file_name.to_string_lossy());
                target_path.set_file_name(new_file_name);
            }
        }
        target_path
    }

    /// 获取原路径
    pub fn get_source_path(uri: &Uri, root_uri: &Uri, target_root_uri: &Uri) -> PathBuf {
        let target_path = util::to_file_path(uri);
        let root_path = util::to_file_path(root_uri);
        let target_root_path = util::to_file_path(target_root_uri);
        // 计算相对路径
        let rel_path = target_path
            .strip_prefix(target_root_path)
            .unwrap()
            .to_path_buf();
        // 转换为原路径
        let mut source_path = root_path.join(rel_path);
        if let Some(file_name) = source_path.file_name() {
            let file_name = file_name.to_string_lossy().to_string();
            if file_name.ends_with(".vue.ts") {
                source_path.set_file_name(&file_name[..file_name.len() - 3]);
            }
        }
        source_path
    }
}
