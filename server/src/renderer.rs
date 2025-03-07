mod combined_rendered_results;
pub mod multi_threaded_comment;
mod parse_document;
mod parse_import_path;
mod parse_ts_file;
mod parse_vue_file;
mod render_cache;
mod render_tree;
mod template_compile;

use std::{collections::HashMap, env::consts::OS, io::Error, path::PathBuf};

use lsp_textdocument::FullTextDocument;
use render_cache::RenderCache;
use render_tree::{InitRenderCache, RenderTree};
use tokio::{
    fs::{self, File},
    io::AsyncReadExt,
};
use tower_lsp::lsp_types::{DidChangeTextDocumentParams, Position, Range, Url};
use tracing::{error, warn};
use walkdir::WalkDir;

use crate::ast::TsFileExportResult;

/// # 渲染器
/// 将项目渲染到同目录下的加上 `.~$` 前缀的目录中
pub struct Renderer {
    root_uri_target_uri: Option<(Url, Url)>,
    alias: HashMap<String, String>,
    render_cache: HashMap<Url, RenderCache>,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            root_uri_target_uri: None,
            alias: HashMap::new(),
            render_cache: HashMap::new(),
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

    pub fn get_render_insert_offset(&self, uri: &Url) -> Option<usize> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(cache.render_insert_offset)
        } else {
            None
        }
    }
}

/// init
impl Renderer {
    /// 创建渲染目录，并进行渲染
    pub async fn init(&mut self, root_uri: &Url) {
        let src_path = root_uri.to_file_path().unwrap();
        // 在当前项目所在的目录创建增加了 `.~$` 前缀的同名目录
        let mut target_root_path = src_path.clone();
        target_root_path.pop();
        let project_name = src_path.file_name().unwrap().to_str().unwrap();
        target_root_path.push(format!(".~${}", project_name));
        if target_root_path.exists() {
            fs::remove_dir_all(&target_root_path).await.unwrap();
        }
        fs::create_dir_all(&target_root_path).await.unwrap();

        self.init_tsconfig_paths(root_uri).await;

        let node_modules_src_path = src_path.join("node_modules");
        let node_modules_target_path = target_root_path.join("node_modules");

        let target_root_uri = Url::from_file_path(target_root_path).unwrap();
        self.render_to_target(root_uri, &target_root_uri).await;

        self.root_uri_target_uri = Some((root_uri.clone(), target_root_uri));

        // 创建 node_modules 的链接
        if node_modules_src_path.exists() {
            fs::symlink(node_modules_src_path, node_modules_target_path)
                .await
                .unwrap();
        }
    }

    /// 从项目目录获取 tsconfig.json 并从中获取别名映射关系
    async fn init_tsconfig_paths(&mut self, root_uri: &Url) -> Option<()> {
        let root_path = root_uri.to_file_path().unwrap();
        let tsconfig_path = root_path.join("tsconfig.json");
        if tsconfig_path.exists() {
            match File::open(tsconfig_path).await {
                Ok(mut file) => {
                    let mut content = String::new();
                    file.read_to_string(&mut content).await.unwrap();
                    self.alias = parse_import_path::parse_alias(&content, root_uri);
                }
                Err(err) => {
                    error!("Read tsconfig.json error: {}", err);
                }
            }
        }
        None
    }

    /// 读取目录下的文件，并渲染到目标目录
    async fn render_to_target(&self, root_uri: &Url, target_root_uri: &Url) {
        let root_path = root_uri.to_file_path().unwrap();
        let mut render_tree = RenderTree::new();
        // 遍历目录
        for entry in WalkDir::new(root_path.clone())
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| {
                !e.file_name()
                    .to_str()
                    .map(|s| s.starts_with(".git") || s == "node_modules")
                    .unwrap_or(false)
            })
        {
            if let Ok(entry) = entry {
                let src_path = entry.path();
                let uri = Url::from_file_path(src_path).unwrap();
                let target_path = Renderer::get_target_path(&uri, root_uri, target_root_uri);

                // 如果父目录不存在，先创建父目录
                if let Some(parent) = target_path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent).await.unwrap();
                    }
                }
                if src_path.is_file() {
                    // 如果是 vue 文件，进行初步解析
                    if src_path.extension().is_some_and(|v| v == "vue") {
                        // 获取文档
                        if let Ok(document) = Renderer::get_document_from_file(
                            &Url::from_file_path(src_path).unwrap(),
                        )
                        .await
                        {
                            let result = parse_vue_file::init_parse_vue_file(&document);
                            if let Some((cache, extends_component)) = result {
                                let mut extends_uri = None;
                                let mut export_name = None;
                                if let Some(extends_component) = extends_component {
                                    extends_uri =
                                        self.get_uri_from_path(&uri, &extends_component.path);
                                    export_name = extends_component.name;
                                }
                                render_tree.add_node(uri, cache, extends_uri.clone());
                                let extends_list =
                                    self.get_extends_list(extends_uri, export_name).await;
                                for (uri, extends_uri) in extends_list {
                                    render_tree.add_node(
                                        uri,
                                        InitRenderCache::TsTransfer(
                                            src_path.to_string_lossy().to_string(),
                                        ),
                                        extends_uri,
                                    );
                                }
                            } else {
                                render_tree.add_node(uri, InitRenderCache::ResolveError, None);
                            }
                        }
                    } else {
                        // 如果不是 vue 文件，创建硬链接
                        fs::hard_link(src_path, target_path).await.unwrap();

                        // 如果 ts 文件中存在组件定义，那么解析组件继承关系
                        if src_path.extension().is_some_and(|v| v == "ts") {
                            if let Ok(document) = Renderer::get_document_from_file(
                                &Url::from_file_path(src_path).unwrap(),
                            )
                            .await
                            {
                                if let Some((cache, extends_component)) =
                                    parse_ts_file::parse_ts_file(&document).await
                                {
                                    let mut extends_uri = None;
                                    let mut export_name = None;
                                    if let Some(extends_component) = extends_component {
                                        extends_uri =
                                            self.get_uri_from_path(&uri, &extends_component.path);
                                        export_name = extends_component.name;
                                    }
                                    render_tree.add_node(uri, cache, extends_uri.clone());
                                    let extends_list =
                                        self.get_extends_list(extends_uri, export_name).await;
                                    for (uri, extends_uri) in extends_list {
                                        render_tree.add_node(
                                            uri,
                                            InitRenderCache::TsTransfer(
                                                src_path.to_string_lossy().to_string(),
                                            ),
                                            extends_uri,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                warn!("walk error: {:?}", entry.unwrap_err());
            }
        }
        // 从顶层节点开始渲染
        for node in render_tree.roots {
            node.render(vec![], root_uri, target_root_uri);
        }
    }

    /// 从导入路径获取 uri，如果对应的文件不存在，返回 None
    fn get_uri_from_path(&self, base_uri: &Url, path: &str) -> Option<Url> {
        let file_path = parse_import_path::parse_import_path(base_uri, path, &self.alias);

        // 如果文件不存在，那么尝试添加后缀
        if !file_path.exists() {
            if let Some(file_name) = file_path.file_name() {
                let suffix_list = [".d.ts", ".ts"];
                for suffix in suffix_list {
                    let new_file_name = format!("{}{}", file_name.to_str().unwrap(), suffix);
                    let new_file_path = file_path.with_file_name(new_file_name);
                    if new_file_path.exists() {
                        return Some(Url::from_file_path(new_file_path).unwrap());
                    }
                }
            }
            let new_file_path = file_path.join("index.ts");
            if new_file_path.exists() {
                return Some(Url::from_file_path(new_file_path).unwrap());
            }
            None
        } else {
            Some(Url::from_file_path(file_path).unwrap())
        }
    }

    /// 根据继承的 uri 获取继承链
    /// 如果继承 uri 是 ts 文件，那么从 ts 文件中递归寻找导出组件
    async fn get_extends_list(
        &self,
        extends_uri: Option<Url>,
        export_name: Option<String>,
    ) -> Vec<(Url, Option<Url>)> {
        // 继承链
        let mut extends_list_result: Vec<(Url, Option<Url>)> = Vec::new();
        // 可能继承的 uri (extends_uri, export_name, extends_list)
        let mut possible_extends_uri =
            vec![(extends_uri, export_name, extends_list_result.clone())];

        // 对继承的 uri 进行分析
        while let Some((uri, export_name, mut extends_list)) = possible_extends_uri.pop() {
            if let Some(uri) = uri {
                // 如果不是 ts 文件
                if !uri
                    .to_file_path()
                    .unwrap()
                    .extension()
                    .is_some_and(|v| v.to_str() == Some("ts"))
                {
                    if export_name == None {
                        // 增加继承链
                        extends_list_result.append(&mut extends_list);
                        break;
                    } else {
                        continue;
                    }
                }
                let result = parse_ts_file::parse_ts_file_export(&uri, &export_name).await;
                match result {
                    TsFileExportResult::Current => {
                        // 增加继承链
                        extends_list_result.append(&mut extends_list);
                        break;
                    }
                    TsFileExportResult::None => {}
                    TsFileExportResult::Other(path, export_name) => {
                        // 增加继承链
                        if let Some(extends_uri) = self.get_uri_from_path(&uri, &path) {
                            extends_list_result.append(&mut extends_list);
                            possible_extends_uri =
                                vec![(Some(extends_uri.clone()), export_name, vec![])];
                            extends_list_result.push((uri, Some(extends_uri)));
                        }
                    }
                    TsFileExportResult::Possible(path_list) => {
                        for path in path_list {
                            let extends_uri = self.get_uri_from_path(&uri, &path);
                            let mut extends_list = extends_list.clone();
                            extends_list.push((uri.clone(), extends_uri.clone()));
                            possible_extends_uri.push((
                                extends_uri,
                                export_name.clone(),
                                extends_list,
                            ));
                        }
                    }
                }
            }
        }
        extends_list_result
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

/// file change
impl Renderer {
    pub fn did_change(
        &mut self,
        uri: &Url,
        params: &DidChangeTextDocumentParams,
        document: &FullTextDocument,
    ) -> DidChangeTextDocumentParams {
        todo!()
    }

    pub async fn shutdown(&self) {
        todo!()
    }
}

/// mapping
impl Renderer {
    pub fn is_position_valid(&self, uri: &Url, position: &Position) -> bool {
        Renderer::is_position_valid_by_document(self.get_document(uri), position)
    }

    pub fn get_original_position(&self, uri: &Url, position: &Position) -> Option<Position> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            let document = &cache.document;
            let line = document
                .position_at(cache.render_insert_offset as u32 + 1)
                .line;
            if line == position.line {
                let original = self.get_original_offset(uri, position.character as usize)? as u32;
                Some(document.position_at(original))
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_original_range(&self, uri: &Url, range: &Range) -> Option<Range> {
        let start = self.get_original_position(uri, &range.start)?;
        let end = self.get_original_position(uri, &range.end)?;
        Some(Range { start, end })
    }

    pub fn get_mapping_position(&self, uri: &Url, offset: usize) -> Option<Position> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            let document = &cache.document;
            let character = self.get_mapping_character(uri, offset)? as u32;
            let line = document
                .position_at(cache.render_insert_offset as u32 + 1)
                .line;
            Some(Position { line, character })
        } else {
            None
        }
    }

    /// 获取编译前的偏移量，如果不在 template 范围内，返回 None
    fn get_original_offset(&self, uri: &Url, offset: usize) -> Option<usize> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            if cache.mapping.len() == 0 {
                return None;
            }
            let render_offset = cache
                .document
                .position_at(cache.render_insert_offset as u32 + 1)
                .character as usize
                - 1;
            if offset < render_offset {
                return None;
            }
            // 减去 render 函数插入位置的偏移量
            let offset = offset - render_offset;
            let mut start = 0;
            let mut end = cache.mapping.len();
            while start < end {
                let mid = (start + end) / 2;
                let (target, source, len) = cache.mapping[mid];
                if target + len < offset {
                    if start == mid {
                        start += 1;
                    } else {
                        start = mid;
                    }
                } else if target > offset {
                    end = mid;
                } else {
                    return Some(source + offset - target);
                }
            }
        }
        return None;
    }

    /// 获取编译后的所在列的字符位置，如果不在 template 范围内返回 None
    ///
    /// `offset` 是模版上的位置
    fn get_mapping_character(&self, uri: &Url, offset: usize) -> Option<usize> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            let render_offset = cache
                .document
                .position_at(cache.render_insert_offset as u32 + 1)
                .character as usize
                - 1;
            if cache.mapping.len() == 0 {
                return None;
            }
            let mut start = 0;
            let mut end = cache.mapping.len();
            while start < end {
                let mid = (start + end) / 2;
                let (mut target, source, len) = cache.mapping[mid];
                // 加上 render 函数插入位置的偏移量
                target += render_offset;
                if source + len < offset {
                    if start == mid {
                        start += 1;
                    } else {
                        start = mid;
                    }
                } else if source > offset {
                    end = mid;
                } else {
                    return Some(target + offset - source);
                }
            }
        }
        return None;
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
}
