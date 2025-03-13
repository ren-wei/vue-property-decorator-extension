mod combined_rendered_results;
pub mod multi_threaded_comment;
mod parse_document;
mod parse_import_path;
mod parse_lib;
mod parse_script;
mod parse_ts_file;
mod parse_vue_file;
mod render_cache;
mod template_compile;

use std::{collections::HashMap, env::consts::OS, io::Error, path::PathBuf};

use html_languageservice::{
    html_data::{Description, IAttributeData, ITagData},
    parser::html_document::{HTMLDocument, Node},
};
use lsp_textdocument::FullTextDocument;
use parse_script::{ExtendsComponent, RegisterComponent};
use render_cache::{
    ExtendsRelationship, LibRenderCache, RegisterRelationship, Relationship, RenderCache,
    RenderCacheGraph, TransferRelationship, TsComponent, TsRenderCache, VueRenderCache,
};
use tokio::{
    fs::{self, File},
    io::AsyncReadExt,
};
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, MarkupContent, MarkupKind, Position, Range,
    TextDocumentContentChangeEvent, Url,
};
use tracing::{error, warn};
use walkdir::WalkDir;

use crate::tags_provider::ArcTagsProvider;

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

    pub fn get_render_insert_offset(&self, uri: &Url) -> Option<usize> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(cache.render_insert_offset)
        } else {
            None
        }
    }
}

// render
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
        self.root_uri_target_uri = Some((root_uri.clone(), target_root_uri.clone()));
        self.render(root_uri, &target_root_uri).await;

        // 创建 node_modules 的链接
        if node_modules_src_path.exists() {
            fs::symlink(node_modules_src_path, node_modules_target_path)
                .await
                .unwrap();
        }
    }

    /// 当文件内容更改时
    /// * 重新解析当前文件
    /// * 更新继承关系
    /// * 更新注册关系
    /// * 更新继承自当前文件的文件
    pub async fn update(
        &mut self,
        uri: &Url,
        params: DidChangeTextDocumentParams,
        document: &FullTextDocument,
    ) -> DidChangeTextDocumentParams {
        let cache = self.render_cache.get_mut(uri).unwrap();
        match cache {
            RenderCache::VueRenderCache(vue_cache) => {
                // 如果变更超过一个，直接全量更新
                if params.content_changes.len() != 1 {
                    self.render_cache.remove_outgoing_edge(uri);
                    self.create_node(uri).await;
                    let content = self.render_cache.get_node_render_content(uri).unwrap();
                    DidChangeTextDocumentParams {
                        text_document: params.text_document,
                        content_changes: vec![TextDocumentContentChangeEvent {
                            range: None,
                            range_length: None,
                            text: content,
                        }],
                    }
                } else {
                    let change = &params.content_changes[0];
                    let range = change.range.unwrap();
                    let range_length = change.range_length.unwrap() as usize;
                    let range_start = vue_cache.document.offset_at(range.start) as usize;
                    let range_end = vue_cache.document.offset_at(range.end) as usize;
                    // 更新缓存文档
                    vue_cache
                        .document
                        .update(&[change.clone()], document.version());
                    let source = document.get_content(None);
                    // 节点需要增加的偏移量
                    let incremental = change.text.len() as isize - range_length as isize;
                    /// 位移节点
                    fn move_node(node: &mut Node, incremental: isize) {
                        node.start = (node.start as isize + incremental) as usize;
                        if let Some(start_tag_end) = node.start_tag_end {
                            node.start_tag_end =
                                Some((start_tag_end as isize + incremental) as usize);
                        }
                        if let Some(end_tag_start) = node.end_tag_start {
                            node.end_tag_start =
                                Some((end_tag_start as isize + incremental) as usize);
                        }
                        node.end = (node.end as isize + incremental) as usize;
                    }
                    // 1. 如果变更处于 template 节点
                    if vue_cache.template.start < range_start && range_end < vue_cache.template.end
                    {
                        // 重新解析 template 节点
                        let node = parse_document::parse_as_node(
                            document,
                            Some(Range::new(
                                document.position_at(vue_cache.template.start as u32),
                                document.position_at(
                                    (vue_cache.template.end as isize + incremental) as u32,
                                ),
                            )),
                        );
                        // 位移 script 节点和 style 节点
                        move_node(&mut vue_cache.script, incremental);
                        for style in &mut vue_cache.style {
                            move_node(style, incremental);
                        }

                        if let Some(node) = node {
                            vue_cache.template = node;
                            vue_cache.render_insert_offset =
                                (vue_cache.render_insert_offset as isize + incremental) as usize;
                            // 进行模版编译
                            let (template_compile_result, mapping) =
                                template_compile::template_compile(&vue_cache.template, source);
                            vue_cache.template_compile_result = template_compile_result;
                            vue_cache.mapping = mapping;
                            // 组合渲染结果
                            let content = self.render_cache.get_node_render_content(uri).unwrap();
                            return DidChangeTextDocumentParams {
                                text_document: params.text_document,
                                content_changes: vec![TextDocumentContentChangeEvent {
                                    range: None,
                                    range_length: None,
                                    text: content,
                                }],
                            };
                        } else {
                            vue_cache.template.end += incremental as usize;
                            // template 节点解析失败，将变更内容转换为空格后输出
                            return DidChangeTextDocumentParams {
                                text_document: params.text_document.clone(),
                                content_changes: vec![TextDocumentContentChangeEvent {
                                    range: change.range,
                                    range_length: change.range_length,
                                    text: " ".repeat(change.text.len()),
                                }],
                            };
                        }
                    }
                    // 2. 如果变更处于 script 节点
                    if vue_cache
                        .script
                        .start_tag_end
                        .is_some_and(|v| v <= range_start)
                        && vue_cache
                            .script
                            .end_tag_start
                            .is_some_and(|v| range_end < v)
                    {
                        vue_cache.script.end_tag_start = Some(
                            (vue_cache.script.end_tag_start.unwrap() as isize + incremental)
                                as usize,
                        );
                        vue_cache.script.end =
                            (vue_cache.script.end as isize + incremental) as usize;
                        for style in &mut vue_cache.style {
                            move_node(style, incremental);
                        }
                        // 尝试`解析脚本`
                        if let Some((props, render_insert_offset, extends_component, registers)) =
                            parse_script::parse_script(
                                source,
                                vue_cache.script.start_tag_end.unwrap(),
                                vue_cache.script.end_tag_start.unwrap(),
                            )
                        {
                            vue_cache.render_insert_offset = render_insert_offset;
                            vue_cache.props = props;
                            // 处理 extends_component 和 registers
                            self.render_cache.remove_outgoing_edge(uri);
                            self.create_extends_relation(uri, extends_component).await;
                            self.create_registers_relation(uri, registers).await;
                        } else {
                            // 解析失败，位移 render_insert_offset
                            vue_cache.render_insert_offset =
                                (vue_cache.render_insert_offset as isize + incremental) as usize;
                        }

                        return params;
                    }

                    // 3. 如果变更位于 style 节点
                    let mut is_in_style = false;
                    for style in &mut vue_cache.style {
                        if is_in_style {
                            style.start = (style.start as isize + incremental) as usize;
                            if let Some(start_tag_end) = style.start_tag_end {
                                style.start_tag_end =
                                    Some((start_tag_end as isize + incremental) as usize);
                            }
                        }
                        if !is_in_style
                            && style.start_tag_end.is_some_and(|v| v <= range_start)
                            && style.end_tag_start.is_some_and(|v| range_end < v)
                        {
                            is_in_style = true;
                        }
                        if is_in_style {
                            if let Some(end_tag_start) = style.end_tag_start {
                                style.end_tag_start =
                                    Some((end_tag_start as isize + incremental) as usize);
                            }
                            style.end = (style.end as isize + incremental) as usize;
                        }
                    }
                    if is_in_style {
                        return DidChangeTextDocumentParams {
                            text_document: params.text_document,
                            content_changes: vec![TextDocumentContentChangeEvent {
                                range: change.range,
                                range_length: change.range_length,
                                text: " ".repeat(change.text.len()),
                            }],
                        };
                    }

                    // 4. 如果变更处于节点边界，进行全量渲染
                    self.render_cache.remove_outgoing_edge(uri);
                    self.create_node(uri).await;
                    if let Some(content) = self.render_cache.get_node_render_content(uri) {
                        DidChangeTextDocumentParams {
                            text_document: params.text_document,
                            content_changes: vec![TextDocumentContentChangeEvent {
                                range: None,
                                range_length: None,
                                text: content,
                            }],
                        }
                    } else {
                        DidChangeTextDocumentParams {
                            text_document: params.text_document.clone(),
                            content_changes: vec![TextDocumentContentChangeEvent {
                                range: None,
                                range_length: None,
                                text: "".to_string(),
                            }],
                        }
                    }
                }
            }
            RenderCache::TsRenderCache(_) => {
                self.render_cache.remove_outgoing_edge(uri);
                self.create_node(uri).await;
                params
            }
            RenderCache::LibRenderCache(_) => {
                error!("Library node update: {}", uri.path());
                params
            }
            RenderCache::Unknown => {
                self.create_node(uri).await;
                if let Some(content) = self.render_cache.get_node_render_content(uri) {
                    DidChangeTextDocumentParams {
                        text_document: params.text_document,
                        content_changes: vec![TextDocumentContentChangeEvent {
                            range: None,
                            range_length: None,
                            text: content,
                        }],
                    }
                } else {
                    params
                }
            }
        }
    }

    pub fn save(&self, uri: &Url) {
        let (root_uri, target_root_uri) = self.root_uri_target_uri.as_ref().unwrap();
        self.render_cache
            .render_node(uri, root_uri, target_root_uri);
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
    /// 同时构建组件间关系图
    async fn render(&mut self, root_uri: &Url, target_root_uri: &Url) {
        let root_path = root_uri.to_file_path().unwrap();
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
                    if src_path.extension().is_some_and(|v| v == "vue") {
                        // 创建 vue 节点
                        self.create_node(&uri).await;
                    } else {
                        // 如果不是 vue 文件，创建硬链接
                        fs::hard_link(src_path, target_path).await.unwrap();

                        if src_path.extension().is_some_and(|v| v == "ts") {
                            // 创建 ts 节点
                            self.create_node(&uri).await;
                        }
                    }
                }
            } else {
                warn!("walk error: {:?}", entry.unwrap_err());
            }
        }
        // 创建组件库节点
        let library_list = self.library_list.clone();
        for lib_node in &library_list {
            self.create_lib_node(lib_node).await;
        }
        self.render_cache.flush();
        self.render_cache.render(root_uri, target_root_uri);
    }

    /// 创建节点及相关的边
    /// * 如果是 vue 文件，那么创建 vue 节点
    /// * 如果是 ts 文件，那么创建 ts 节点
    /// * 如果都不是或者创建失败，那么创建 Unknown 节点
    async fn create_node(&mut self, uri: &Url) {
        if Renderer::is_vue_component(uri) {
            if self.create_vue_node(uri).await.is_none() {
                self.crate_unknown_node(uri);
            }
        } else {
            if self.create_ts_node(uri).await.is_none() {
                self.crate_unknown_node(uri);
            }
        }
    }

    /// 创建 vue 节点
    /// * 如果存在继承关系，那么创建继承边
    /// * 如果存在注册关系，那么创建注册边
    async fn create_vue_node(&mut self, uri: &Url) -> Option<()> {
        let document = Renderer::get_document_from_file(uri).await.unwrap();
        let result = parse_vue_file::parse_vue_file(&document)?;
        self.render_cache.add_node(
            uri,
            RenderCache::VueRenderCache(VueRenderCache {
                document,
                template: result.template,
                script: result.script,
                style: result.style,
                props: result.props,
                render_insert_offset: result.render_insert_offset,
                template_compile_result: result.template_compile_result,
                mapping: result.mapping,
            }),
        );
        self.create_extends_relation(uri, result.extends_component)
            .await;
        self.create_registers_relation(uri, result.registers).await;
        Some(())
    }

    /// 创建 ts 节点
    /// * 如果存在组件并且存在继承关系，那么创建继承边
    /// * 如果存在组件并且存在注册关系，那么创建注册边
    /// * 创建节点间中转关系
    async fn create_ts_node(&mut self, uri: &Url) -> Option<()> {
        let document = Renderer::get_document_from_file(uri).await.unwrap();
        let result = parse_ts_file::parse_ts_file(&document)?;
        let mut ts_component = None;
        if let Some((props, extends_component, registers)) = result.ts_component {
            ts_component = Some(TsComponent { props });
            self.create_extends_relation(uri, extends_component).await;
            self.create_registers_relation(uri, registers).await;
        };
        self.render_cache.add_node(
            uri,
            RenderCache::TsRenderCache(TsRenderCache {
                ts_component,
                local_exports: result.local_exports,
            }),
        );
        for (local, export_name, path, is_star_export) in result.transfers {
            if let Some(transfer_uri) = self.get_uri_from_path(uri, &path) {
                if Renderer::is_uri_valid(&transfer_uri) {
                    self.render_cache.add_virtual_edge(
                        uri,
                        &transfer_uri,
                        Relationship::TransferRelationship(TransferRelationship {
                            local,
                            export_name,
                            is_star_export,
                        }),
                    );
                }
            }
        }
        Some(())
    }

    fn crate_unknown_node(&mut self, uri: &Url) {
        warn!("unknown node type: {}", uri.path());
        self.render_cache.add_node(uri, RenderCache::Unknown);
    }

    async fn create_lib_node(&mut self, uri: &Url) {
        self.render_cache.add_node(
            uri,
            RenderCache::LibRenderCache(LibRenderCache {
                components: parse_lib::parse_specific_lib(uri),
            }),
        );
    }

    /// 创建继承关系
    async fn create_extends_relation(
        &mut self,
        uri: &Url,
        extends_component: Option<ExtendsComponent>,
    ) {
        if let Some(component) = extends_component {
            let extends_uri = self.get_uri_from_path(uri, &component.path);
            if let Some(extends_uri) = extends_uri {
                if Renderer::is_uri_valid(&extends_uri) {
                    self.render_cache.add_virtual_edge(
                        &uri,
                        &extends_uri,
                        Relationship::ExtendsRelationship(ExtendsRelationship {
                            export_name: component.export_name,
                        }),
                    );
                } else if component.path != "vue" {
                    warn!("Extends path parse fail: {} {}", component.path, uri.path());
                }
            }
        }
    }

    /// 创建注册关系
    async fn create_registers_relation(&mut self, uri: &Url, registers: Vec<RegisterComponent>) {
        for register in registers {
            let register_uri = self.get_uri_from_path(&uri, &register.path);
            if let Some(register_uri) = register_uri {
                if Renderer::is_uri_valid(&register_uri) || Renderer::is_node_modules(&register_uri)
                {
                    if Renderer::is_node_modules(&register_uri)
                        && !self.library_list.contains(&register_uri)
                    {
                        self.library_list.push(register_uri.clone());
                    }
                    self.render_cache.add_virtual_edge(
                        uri,
                        &register_uri,
                        Relationship::RegisterRelationship(RegisterRelationship {
                            registered_name: register.name,
                            export_name: register.export,
                            prop: register.prop,
                        }),
                    );
                } else {
                    warn!("Register path parse fail: {}", register.path);
                }
            }
        }
    }

    /// 从导入路径获取 uri，如果对应的文件不存在，返回 None
    fn get_uri_from_path(&self, base_uri: &Url, path: &str) -> Option<Url> {
        let file_path = parse_import_path::parse_import_path(
            base_uri,
            path,
            &self.alias,
            &self.root_uri_target_uri.as_ref().unwrap().0,
        );
        if file_path.is_dir() && file_path.to_string_lossy().contains("/node_modules/") {
            return Some(Url::from_file_path(file_path).unwrap());
        }

        // 如果文件不存在，那么尝试添加后缀
        if !file_path.is_file() {
            if let Some(file_name) = file_path.file_name() {
                let suffix_list = [".d.ts", ".ts"];
                for suffix in suffix_list {
                    let new_file_name = format!("{}{}", file_name.to_str().unwrap(), suffix);
                    let new_file_path = file_path.with_file_name(new_file_name);
                    if new_file_path.is_file() {
                        return Some(Url::from_file_path(new_file_path).unwrap());
                    }
                }
            }
            let new_file_path = file_path.join("index.ts");
            if new_file_path.is_file() {
                return Some(Url::from_file_path(new_file_path).unwrap());
            }
            None
        } else {
            Some(Url::from_file_path(file_path).unwrap())
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
                .line
                + 1;
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
                .line
                + 1;
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
            if cache.mapping.len() == 0 {
                return None;
            }
            let mut start = 0;
            let mut end = cache.mapping.len();
            while start < end {
                let mid = (start + end) / 2;
                let (target, source, len) = cache.mapping[mid];
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

    /// 获取 vue 组件所处位置的类型
    pub fn get_position_type(&self, uri: &Url, position: &Position) -> Option<PositionType> {
        let cache = &self.render_cache[uri];
        if let RenderCache::VueRenderCache(cache) = cache {
            let offset = cache.document.offset_at(*position) as usize;
            if cache.template.start < offset && offset < cache.template.end {
                if let Some(pos) = self.get_mapping_position(uri, offset) {
                    Some(PositionType::TemplateExpr(pos))
                } else {
                    Some(PositionType::Template)
                }
            } else if cache.script.start_tag_end.unwrap() < offset
                && offset < cache.script.end_tag_start.unwrap()
            {
                Some(PositionType::Script)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// tags
impl Renderer {
    /// 获取 provider，如果不是最新则先更新
    pub async fn get_tags_provider(&mut self, uri: &Url) -> ArcTagsProvider {
        let version = self.get_document_version(uri);
        if let Some(provider) = self.provider_map.get(uri) {
            if provider.version() == version {
                return provider.clone();
            }
        }
        let mut tags = vec![];
        // 获取当前节点注册的组件
        let registers = self.render_cache.get_registers(uri);
        for (register_name, export_name, prop, cache) in registers {
            match cache {
                RenderCache::VueRenderCache(cache) => {
                    tags.push(ITagData {
                        name: register_name.clone(),
                        description: Some(Description::MarkupContent(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("```typescript\nclass {}\n```", register_name),
                        })),
                        attributes: cache
                            .props
                            .iter()
                            .map(|prop| IAttributeData {
                                name: prop.clone(),
                                description: None,
                                value_set: None,
                                values: None,
                                references: None,
                            })
                            .collect(),
                        references: None,
                        void: None,
                    });
                }
                RenderCache::TsRenderCache(cache) => {
                    if let Some(ts_component) = &cache.ts_component {
                        tags.push(ITagData {
                            name: register_name.clone(),
                            description: Some(Description::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!("```typescript\nclass {}\n```", register_name),
                            })),
                            attributes: ts_component
                                .props
                                .iter()
                                .map(|prop| IAttributeData {
                                    name: prop.clone(),
                                    description: None,
                                    value_set: None,
                                    values: None,
                                    references: None,
                                })
                                .collect(),
                            references: None,
                            void: None,
                        });
                    }
                }
                RenderCache::LibRenderCache(lib_cache) => {
                    // 从组件库节点获取标签定义
                    let component = lib_cache
                        .components
                        .iter()
                        .find(|c| export_name.as_ref().is_some_and(|v| *v == c.name));
                    if let Some(mut component) = component {
                        if let Some(prop) = prop {
                            let target = component.static_props.iter().find(|c| c.name == prop);
                            if let Some(target) = target {
                                component = target.as_ref();
                            } else {
                                continue;
                            }
                        }
                        tags.push(ITagData {
                            name: register_name.clone(),
                            description: Some(Description::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!("```typescript\nclass {}\n```", register_name),
                            })),
                            attributes: component
                                .props
                                .iter()
                                .map(|prop| IAttributeData {
                                    name: prop.name.clone(),
                                    description: None,
                                    value_set: None,
                                    values: None,
                                    references: None,
                                })
                                .collect(),
                            references: None,
                            void: None,
                        });
                    }
                }
                RenderCache::Unknown => {}
            }
        }
        // TODO: 获取继承节点注册的组件
        let provider = ArcTagsProvider::new(uri.path().to_string(), tags, version);
        self.provider_map.insert(uri.clone(), provider.clone());
        provider
    }

    pub fn get_document_version(&self, uri: &Url) -> Option<i32> {
        let cache = &self.render_cache[uri];
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(cache.document.version())
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
}

#[derive(PartialEq, Debug)]
pub enum PositionType {
    Script,
    Template,
    TemplateExpr(Position),
}
