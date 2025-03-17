use std::path::PathBuf;

use html_languageservice::parser::html_document::Node;
use lsp_textdocument::FullTextDocument;
use swc_common::source_map::SmallPos;
use tokio::{
    fs::{self, File},
    io::AsyncReadExt,
};
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, Range, TextDocumentContentChangeEvent, Url,
};
use tracing::{error, warn};
use walkdir::WalkDir;

use crate::renderer::{
    parse_document,
    parse_script::{self, ParseScriptResult},
    template_compile,
};

use super::{
    parse_import_path, parse_lib,
    parse_script::{ExtendsComponent, RegisterComponent},
    parse_ts_file, parse_vue_file,
    render_cache::{
        ExtendsRelationship, LibRenderCache, RegisterRelationship, Relationship, RenderCache,
        TransferRelationship, TsComponent, TsRenderCache, VueRenderCache,
    },
    Renderer,
};

pub trait Render {
    async fn init(&mut self, root_uri: &Url);
    async fn update(
        &mut self,
        uri: &Url,
        params: DidChangeTextDocumentParams,
        document: &FullTextDocument,
    ) -> DidChangeTextDocumentParams;
    fn save(&self, uri: &Url);
}

impl Render for Renderer {
    /// 创建渲染目录，并进行渲染
    async fn init(&mut self, root_uri: &Url) {
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
            if cfg!(target_os = "windows") {
                fn copy_dir(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
                    // 创建目标目录
                    std::fs::create_dir_all(dst)?;

                    // 使用 WalkDir 遍历源目录
                    for entry in WalkDir::new(src) {
                        let entry = entry?;
                        let src_path = entry.path();
                        let relative_path = src_path.strip_prefix(src).unwrap();
                        let dst_path = dst.join(relative_path);

                        if src_path.is_dir() {
                            // 如果是目录，创建对应的目标目录
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
                copy_dir(&node_modules_src_path, &node_modules_target_path).unwrap();
            } else {
                fs::symlink(node_modules_src_path, node_modules_target_path)
                    .await
                    .unwrap();
            }
        }
    }

    /// 当文件内容更改时
    /// * 重新解析当前文件
    /// * 更新继承关系
    /// * 更新注册关系
    /// * 更新继承自当前文件的文件
    async fn update(
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
                        if let Some(ParseScriptResult {
                            name_span,
                            description,
                            props,
                            render_insert_offset,
                            extends_component,
                            registers,
                        }) = parse_script::parse_script(
                            source,
                            vue_cache.script.start_tag_end.unwrap(),
                            vue_cache.script.end_tag_start.unwrap(),
                        ) {
                            vue_cache.render_insert_offset = render_insert_offset;
                            vue_cache.name_range = Range {
                                start: document.position_at(name_span.lo.to_u32()),
                                end: document.position_at(name_span.hi.to_u32()),
                            };
                            vue_cache.description = description;
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

    fn save(&self, uri: &Url) {
        let (root_uri, target_root_uri) = self.root_uri_target_uri.as_ref().unwrap();
        self.render_cache
            .render_node(uri, root_uri, target_root_uri);
    }
}

impl Renderer {
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
                name_range: result.name_range,
                description: result.description,
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
        if let Some((name_range, description, props, extends_component, registers)) =
            result.ts_component
        {
            ts_component = Some(TsComponent {
                name_range,
                description,
                props,
            });
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
}
