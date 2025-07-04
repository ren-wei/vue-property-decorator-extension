#[cfg(target_os = "windows")]
use std::path::PathBuf;

use lsp_textdocument::FullTextDocument;
use tokio::{
    fs::{self, File},
    io::AsyncReadExt,
};
use tower_lsp::{
    lsp_types::{
        DidChangeTextDocumentParams, ProgressToken, TextDocumentContentChangeEvent, Uri,
        VersionedTextDocumentIdentifier,
    },
    Client,
};
#[cfg(target_os = "windows")]
use tower_lsp::{NotCancellable, OngoingProgress, Unbounded};
use tracing::{error, warn};
use walkdir::WalkDir;

use crate::util;

use super::{
    parse_import_path,
    parse_script::{ExtendsComponent, RegisterComponent},
    render_cache::{
        lib_render_cache,
        ts_render_cache::{self, TsComponent, TsRenderCache},
        vue_render_cache::{self, VueRenderCache},
        ExtendsRelationship, RegisterRelationship, Relationship, RenderCache, TransferRelationship,
    },
    Renderer,
};

impl Renderer {
    /// 创建渲染目录，并进行渲染
    pub async fn init(&mut self, root_uri: &Uri, client: &Client, work_done_token: ProgressToken) {
        let progress = client
            .progress(work_done_token, "Vue2 Language Server")
            .begin()
            .await;
        let src_path = util::to_file_path(root_uri);
        // 在当前项目所在的目录创建增加了 `.~$` 前缀的同名目录
        let mut target_root_path = src_path.clone();
        target_root_path.pop();
        let project_name = src_path.file_name().unwrap().to_str().unwrap();
        target_root_path.push(format!(".~${}", project_name));
        // windows 下，如果目标目录已经存在，那么跳过删除和重新复制 node_modules
        #[cfg(target_os = "windows")]
        let skip = target_root_path.exists();
        #[cfg(not(target_os = "windows"))]
        if target_root_path.exists() {
            fs::remove_dir_all(&target_root_path).await.unwrap();
        }
        #[cfg(target_os = "windows")]
        if !skip {
            fs::create_dir_all(&target_root_path).await.unwrap();
        }
        #[cfg(not(target_os = "windows"))]
        fs::create_dir_all(&target_root_path).await.unwrap();

        self.init_tsconfig_paths(root_uri).await;

        let node_modules_src_path = src_path.join("node_modules");
        let node_modules_target_path = target_root_path.join("node_modules");

        let target_root_uri = util::create_uri_from_path(&target_root_path);
        self.root_uri_target_uri
            .set((root_uri.clone(), target_root_uri.clone()))
            .unwrap();
        progress.report("Initializing...").await;
        self.render(root_uri, &target_root_uri).await;

        // 创建 node_modules 的链接
        if node_modules_src_path.exists() {
            #[cfg(not(target_os = "windows"))]
            fs::symlink(&node_modules_src_path, &node_modules_target_path)
                .await
                .unwrap();
            #[cfg(target_os = "windows")]
            if !skip {
                async fn copy_dir(
                    src: &PathBuf,
                    dst: &PathBuf,
                    progress: &OngoingProgress<Unbounded, NotCancellable>,
                ) -> std::io::Result<()> {
                    // 创建目标目录
                    std::fs::create_dir_all(dst)?;

                    // 使用 WalkDir 遍历源目录
                    for entry in WalkDir::new(src).into_iter().filter_entry(|e| {
                        !e.file_name()
                            .to_str()
                            .map(|s| s == ".cache" || s == ".bin")
                            .unwrap_or(false)
                    }) {
                        let entry = entry?;
                        let src_path = entry.path();
                        let relative_path = src_path.strip_prefix(src).unwrap();
                        let dst_path = dst.join(relative_path);

                        if src_path.is_dir() {
                            // 如果是目录，创建对应的目标目录
                            if dst_path.parent().unwrap() == dst {
                                progress
                                    .report(format!(
                                        "Loading: node_modules/{}",
                                        dst_path.file_name().unwrap().to_string_lossy()
                                    ))
                                    .await;
                            }
                            std::fs::create_dir_all(&dst_path)?;
                        } else {
                            // 如果是文件，复制文件
                            match std::fs::copy(&src_path, &dst_path) {
                                Ok(_) => {}
                                Err(ref e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                                    // 如果目标文件已存在，可根据需求进行处理，这里简单忽略
                                }
                                Err(e) => return Err(e),
                            }
                        }
                    }
                    Ok(())
                }
                copy_dir(&node_modules_src_path, &node_modules_target_path, &progress)
                    .await
                    .unwrap();
            }
        }
        progress.finish().await;
    }

    /// 当文件内容更改时
    /// * 更新当前文件
    /// * 更新继承关系
    /// * 更新注册关系
    /// * 更新继承自当前文件的文件
    pub fn update(
        &mut self,
        uri: &Uri,
        params: DidChangeTextDocumentParams,
        document: &FullTextDocument,
    ) -> DidChangeTextDocumentParams {
        let mut content_changes = vec![];
        for change in &params.content_changes {
            let cache = self.render_cache.get_mut(uri).unwrap();
            let result = cache.update(change.clone());
            if let Some(mut result) = result {
                // 更新影响的组件的版本
                if result.is_change {
                    self.render_cache.update_incoming_node_version(uri);
                }
                // 更新继承关系
                if let Some(extends_component) = result.extends_component {
                    self.render_cache.remove_extends_edge(uri);
                    self.create_extends_relation(uri, extends_component);
                }
                // 更新注册关系
                if let Some(registers) = result.registers {
                    self.render_cache.remove_registers_edges(uri);
                    self.create_registers_relation(uri, registers);
                }
                // 更新转换关系
                if let Some(transfers) = result.transfers {
                    self.render_cache.remove_transfers_edges(uri);
                    self.create_transfers_relation(uri, transfers);
                }
                content_changes.append(&mut result.changes);
                self.render_cache.flush();
            } else {
                // 重新解析节点
                self.render_cache.remove_outgoing_edge(uri);
                self.create_node_from_document(
                    uri,
                    FullTextDocument::new(
                        document.language_id().to_string(),
                        self.render_cache
                            .get(uri)
                            .unwrap()
                            .get_version()
                            .unwrap_or(document.version()),
                        document.get_content(None).to_string(),
                    ),
                );
                self.render_cache.flush();
                if let Some(content) = self.render_cache.get_node_render_content(uri) {
                    return DidChangeTextDocumentParams {
                        text_document: params.text_document,
                        content_changes: vec![TextDocumentContentChangeEvent {
                            range: None,
                            range_length: None,
                            text: content,
                        }],
                    };
                } else {
                    return params;
                }
            }
        }
        DidChangeTextDocumentParams {
            text_document: params.text_document,
            content_changes,
        }
    }

    /// 保存 vue 节点，重新全量渲染，返回变更内容
    pub async fn save(&mut self, uri: &Uri) -> Option<DidChangeTextDocumentParams> {
        // 保存前再次全量解析 vue 节点为 update 出错提供修复机会
        let version = self.render_cache.get(uri)?.get_version()?;
        self.render_cache.remove_outgoing_edge(uri);
        self.create_node(uri).await;
        self.render_cache.flush();
        self.render_cache
            .get_mut(uri)
            .unwrap()
            .update_version(version + 1);
        // 更新影响的组件
        self.render_cache.update_incoming_node_version(uri);
        let change = if let Some(content) = self.render_cache.get_node_render_content(uri) {
            Some(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: content,
                }],
            })
        } else {
            None
        };

        if !cfg!(test) {
            let (root_uri, target_root_uri) = self.root_uri_target_uri.get().unwrap();
            self.render_cache
                .render_node(uri, root_uri, target_root_uri);
        }

        change
    }

    /// 是否需要等待文件创建
    pub fn is_wait_create(&self, uri: &Uri) -> bool {
        self.will_create_files.contains(uri)
    }

    /// 文件打开时检查节点是否存在，如果节点不存在，那么先创建节点
    pub async fn did_open(&mut self, uri: &Uri) {
        if self.render_cache.get(uri).is_none() {
            let (root_uri, target_root_uri) = self.root_uri_target_uri.get().unwrap().clone();
            self.create_node(uri).await;
            self.render_cache.flush();
            self.render_cache
                .render_node(uri, &root_uri, &target_root_uri);
        }
    }

    pub fn will_create_files(&mut self, uris: Vec<Uri>) {
        for uri in uris {
            if Renderer::is_uri_valid(&uri) {
                self.will_create_files.insert(uri);
            }
        }
    }

    pub async fn did_create_files(&mut self, uris: Vec<Uri>) {
        let (root_uri, target_root_uri) = self.root_uri_target_uri.get().unwrap().clone();
        for uri in uris {
            if Renderer::is_uri_valid(&uri) {
                self.create_node(&uri).await;
                self.render_cache
                    .render_node(&uri, &root_uri, &target_root_uri);
                self.will_create_files.remove(&uri);
            }
        }
        self.render_cache.flush();
    }

    pub fn did_delete_files(&mut self, uris: Vec<Uri>) {
        let (root_uri, target_root_uri) = self.root_uri_target_uri.get().unwrap().clone();
        for uri in uris {
            if self.render_cache.get(&uri).is_some() {
                self.render_cache.update_incoming_node_version(&uri);
                self.render_cache
                    .remove_node(&uri, &root_uri, &target_root_uri);
            }
        }
    }

    pub async fn clean_cache_and_restart(
        &mut self,
        client: &Client,
        work_done_token: ProgressToken,
    ) {
        let (root_uri, target_root_uri) = self.root_uri_target_uri.get().unwrap().clone();
        let target_root_path = util::to_file_path(&target_root_uri);
        fs::remove_dir_all(&target_root_path).await.unwrap();
        self.init(&root_uri, client, work_done_token).await;
    }
}

impl Renderer {
    /// 从项目目录获取 tsconfig.json 并从中获取别名映射关系
    async fn init_tsconfig_paths(&mut self, root_uri: &Uri) -> Option<()> {
        let root_path = util::to_file_path(root_uri);
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
    async fn render(&mut self, root_uri: &Uri, target_root_uri: &Uri) {
        let root_path = util::to_file_path(root_uri);
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
                let uri = util::create_uri_from_path(src_path);
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
                        if target_path.exists() {
                            fs::remove_file(&target_path).await.unwrap();
                        }
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
            self.create_lib_node(lib_node);
        }
        self.render_cache.flush();
        self.render_cache.render(root_uri, target_root_uri);
    }

    /// 创建节点及相关的边
    /// * 如果是 vue 文件，那么创建 vue 节点
    /// * 如果是 ts 文件，那么创建 ts 节点
    /// * 如果都不是或者创建失败，那么创建 Unknown 节点
    async fn create_node(&mut self, uri: &Uri) {
        let document = Renderer::get_document_from_file(uri).await.unwrap();
        if Renderer::is_vue_component(uri) {
            self.create_vue_node(uri, document);
        } else {
            self.create_ts_node(uri, document);
        }
    }

    /// 创建节点及相关的边
    pub fn create_node_from_document(&mut self, uri: &Uri, document: FullTextDocument) {
        if Renderer::is_vue_component(uri) {
            self.create_vue_node(uri, document);
        } else {
            self.create_ts_node(uri, document);
        }
    }

    /// 创建 vue 节点
    /// * 如果存在继承关系，那么创建继承边
    /// * 如果存在注册关系，那么创建注册边
    fn create_vue_node(&mut self, uri: &Uri, document: FullTextDocument) {
        let result = vue_render_cache::parse_vue_file(&document);
        self.render_cache.add_node(
            uri,
            RenderCache::VueRenderCache(VueRenderCache {
                document,
                template: result.template,
                script: result.script,
                style: result.style,
                name_range: result.name_range,
                description: result.description,
                props: result.props,
                render_insert_offset: result.render_insert_offset,
                template_compile_result: FullTextDocument::new(
                    "typescript".to_string(),
                    0,
                    result.template_compile_result,
                ),
                mapping: result.mapping,
                safe_update_range: result.safe_update_range,
            }),
        );
        self.create_extends_relation(uri, result.extends_component);
        self.create_registers_relation(uri, result.registers);
    }

    /// 创建 ts 节点
    /// * 如果存在组件并且存在继承关系，那么创建继承边
    /// * 如果存在组件并且存在注册关系，那么创建注册边
    /// * 创建节点间中转关系
    fn create_ts_node(&mut self, uri: &Uri, document: FullTextDocument) {
        let result = ts_render_cache::parse_ts_file(&document);
        let mut ts_component = None;
        if let Some((name_range, description, props, extends_component, registers)) =
            result.ts_component
        {
            ts_component = Some(TsComponent {
                name_range,
                description,
                props,
            });
            self.create_extends_relation(uri, extends_component);
            self.create_registers_relation(uri, registers);
        };
        self.render_cache.add_node(
            uri,
            RenderCache::TsRenderCache(TsRenderCache {
                document,
                ts_component,
                local_exports: result.local_exports,
            }),
        );
        self.create_transfers_relation(uri, result.transfers);
    }

    fn create_lib_node(&mut self, uri: &Uri) {
        self.render_cache.add_node(
            uri,
            RenderCache::LibRenderCache(lib_render_cache::parse_specific_lib(uri)),
        );
    }

    /// 创建继承关系
    fn create_extends_relation(&mut self, uri: &Uri, extends_component: Option<ExtendsComponent>) {
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
    fn create_registers_relation(&mut self, uri: &Uri, registers: Vec<RegisterComponent>) {
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

    /// 更新转换关系
    fn create_transfers_relation(
        &mut self,
        uri: &Uri,
        transfers: Vec<(Option<String>, Option<String>, String, bool)>,
    ) {
        for (local, export_name, path, is_star_export) in transfers {
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
    }

    /// 从导入路径获取 uri，如果对应的文件不存在，返回 None
    #[cfg(not(test))]
    fn get_uri_from_path(&self, base_uri: &Uri, path: &str) -> Option<Uri> {
        let file_path = parse_import_path::parse_import_path(
            base_uri,
            path,
            &self.alias,
            &self.root_uri_target_uri.get().unwrap().0,
        );
        if file_path.is_dir() && file_path.to_string_lossy().contains("/node_modules/") {
            return Some(util::create_uri_from_path(&file_path));
        }

        // 如果文件不存在，那么尝试添加后缀
        if !file_path.is_file() {
            if let Some(file_name) = file_path.file_name() {
                let suffix_list = [".d.ts", ".ts"];
                for suffix in suffix_list {
                    let new_file_name = format!("{}{}", file_name.to_str().unwrap(), suffix);
                    let new_file_path = file_path.with_file_name(new_file_name);
                    if new_file_path.is_file() {
                        return Some(util::create_uri_from_path(&new_file_path));
                    }
                }
            }
            let new_file_path = file_path.join("index.ts");
            if new_file_path.is_file() {
                return Some(util::create_uri_from_path(&new_file_path));
            }
            None
        } else {
            Some(util::create_uri_from_path(&file_path))
        }
    }

    #[cfg(test)]
    /// 从导入路径获取 uri，如果对应的文件不存在，返回 None
    fn get_uri_from_path(&self, base_uri: &Uri, path: &str) -> Option<Uri> {
        let file_path = parse_import_path::parse_import_path(
            base_uri,
            path,
            &self.alias,
            &self.root_uri_target_uri.get().unwrap().0,
        );
        Some(util::create_uri_from_path(&file_path))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        str::FromStr,
    };

    use tokio::sync::OnceCell;
    use tower_lsp::lsp_types::{Location, Position, Range, Uri};

    use crate::{
        renderer::{
            render_cache::{RenderCache, RenderCacheGraph},
            Renderer,
        },
        util,
    };

    #[test]
    fn parse_lib() {
        let exe_path = std::env::current_exe().unwrap();
        let mut path = exe_path.parent().unwrap().to_path_buf();
        while !path.file_name().is_some_and(|name| name == "server") {
            path = path.parent().unwrap().to_path_buf();
        }
        path = path.parent().unwrap().to_path_buf();

        let mut lib_path = path.clone();
        lib_path.push("node_modules/element-ui");

        let lib_uri = util::create_uri_from_path(&lib_path);

        let cache_graph = RenderCacheGraph::new();
        let mut renderer = Renderer {
            root_uri_target_uri: OnceCell::from((
                Uri::from_str("file:///path/project").unwrap(),
                Uri::from_str("file:///path/.~$project").unwrap(),
            )),
            alias: HashMap::new(),
            render_cache: cache_graph,
            provider_map: HashMap::new(),
            library_list: vec![],
            will_create_files: HashSet::new(),
        };
        renderer.create_lib_node(&lib_uri);
        let lib_cache = renderer.render_cache.get(&lib_uri).unwrap();
        assert!(lib_cache.is_lib());
        if let RenderCache::LibRenderCache(lib_cache) = lib_cache {
            let alert = lib_cache.components.iter().find(|v| v.name == "ElAlert");
            let alert = alert.unwrap();
            assert!(alert.description.is_some());
            assert!(alert.props.len() > 0);
            let title = alert.props.iter().find(|v| v.name == "title").unwrap();
            path.push("node_modules/element-ui/types/alert.d.ts");
            assert_eq!(
                title.location,
                Location {
                    uri: util::create_uri_from_path(&path),
                    range: Range {
                        start: Position {
                            line: 8,
                            character: 2,
                        },
                        end: Position {
                            line: 8,
                            character: 15
                        }
                    }
                }
            )
        }
    }
}
