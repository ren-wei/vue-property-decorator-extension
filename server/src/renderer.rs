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
use std::{collections::HashMap, io::Error, path::PathBuf};

use lsp_textdocument::FullTextDocument;
use tower_lsp::lsp_types::{Position, Range, Uri};
use tracing::error;

pub use combined_rendered_results::get_fill_space_source;

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

        let path = util::to_file_path_string(uri);
        match File::open(&path).await {
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
        let language_id = path[path.rfind(".").unwrap() + 1..].to_string();
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
    /// * 不在 .git 中
    pub fn is_uri_valid(uri: &Uri) -> bool {
        let file_path = util::to_file_path(uri);
        if cfg!(not(test)) {
            let file_path_str = file_path.to_string_lossy();
            file_path.is_file()
                && !file_path_str.contains("/node_modules/")
                && !file_path_str.contains("/.git/")
        } else {
            !file_path.to_string_lossy().contains("/node_modules/")
        }
    }

    /// uri 是否指向 node_modules 下的库
    /// * 是目录
    /// * 存在于文件系统中
    pub fn is_node_modules(uri: &Uri) -> bool {
        let file_path = util::to_file_path(uri);
        if cfg!(not(test)) {
            file_path.is_dir() && file_path.to_string_lossy().contains("/node_modules/")
        } else {
            file_path.to_string_lossy().contains("/node_modules/")
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

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        str::FromStr,
    };

    use lsp_textdocument::FullTextDocument;
    use tower_lsp::lsp_types::{
        DidChangeTextDocumentParams, Position, Range, TextDocumentContentChangeEvent, Uri,
        VersionedTextDocumentIdentifier,
    };

    use crate::renderer::{render_cache::RenderCacheGraph, Renderer};
    use lazy_static::lazy_static;

    use super::PositionType;

    lazy_static! {
        static ref TEST1_INDEX: Uri =
            Uri::from_str("file:///path/project/src/test1/index.vue").unwrap();
        static ref TEST1_COMPONENT1: Uri =
            Uri::from_str("file:///path/project/src/test1/components/MyComponent1.vue").unwrap();
        static ref TEST1_COMPONENT2: Uri =
            Uri::from_str("file:///path/project/src/test1/components/MyComponent2.vue").unwrap();
        static ref TEST1_COMPONENT3: Uri =
            Uri::from_str("file:///path/project/src/test1/components/MyComponent3.vue").unwrap();
        static ref TEST2_INDEX: Uri =
            Uri::from_str("file:///path/project/src/test2/index.vue").unwrap();
        static ref TEST2_TS: Uri =
            Uri::from_str("file:///path/project/src/test2/ts/transfer.ts").unwrap();
        static ref TEST2_COMPONENT4: Uri =
            Uri::from_str("file:///path/project/src/test2/components/MyComponent4.vue").unwrap();
        static ref TEST2_COMPONENT5: Uri =
            Uri::from_str("file:///path/project/src/test2/components/MyComponent5.vue").unwrap();
    }

    fn create_renderer() -> Renderer {
        let cache_graph = RenderCacheGraph::new();
        let mut renderer = Renderer {
            root_uri_target_uri: Some((
                Uri::from_str("file:///path/project").unwrap(),
                Uri::from_str("file:///path/.~$project").unwrap(),
            )),
            alias: HashMap::new(),
            render_cache: cache_graph,
            provider_map: HashMap::new(),
            library_list: vec![],
            will_create_files: HashSet::new(),
        };
        // test1/index.vue
        renderer.create_node_from_document(
            &TEST1_INDEX,
            FullTextDocument::new(
                "vue".to_string(),
                0,
                [
                    "<template>",
                    "  <MyComponent1 title=\"Title\" />",
                    "</template>",
                    "<script lang=\"ts\">",
                    "import Vue from 'vue';",
                    "import { Component } from 'vue-property-decorator';",
                    "import MyComponent1 from './components/MyComponent1.vue';",
                    "@Component({",
                    "  components: {",
                    "    MyComponent1,",
                    "  },",
                    "})",
                    "export default class Index extends Vue {",
                    "}",
                    "</script>",
                ]
                .join("\n")
                .to_string(),
            ),
        );
        // test1/components/MyComponent1.vue
        renderer.create_node_from_document(
            &TEST1_COMPONENT1,
            FullTextDocument::new(
                "vue".to_string(),
                0,
                [
                    "<template>",
                    "  <div :title=\"title\" :tabIndex=\"\">{{ text }}</div>",
                    "</template>",
                    "<script lang=\"ts\">",
                    "import { Component } from 'vue-property-decorator';",
                    "import MyComponent2 from './MyComponent2.vue';",
                    "@Component",
                    "export default class MyComponent1 extends MyComponent2 {",
                    "  @Prop({ type: String, required: true })",
                    "  private title!: string;",
                    "  private text = 'Hello World';",
                    "}",
                    "</script>",
                ]
                .join("\n")
                .to_string(),
            ),
        );
        // test1/components/MyComponent2.vue
        renderer.create_node_from_document(
            &TEST1_COMPONENT2,
            FullTextDocument::new(
                "vue".to_string(),
                0,
                [
                    "<template>",
                    "  <div></div>",
                    "</template>",
                    "<script lang=\"ts\">",
                    "import Vue from 'vue';",
                    "import { Component } from 'vue-property-decorator';",
                    "@Component",
                    "export default class MyComponent2 extends Vue {",
                    "  @Prop({ type: Boolean, default: false })",
                    "  private readonly!: boolean;",
                    "  private state = 'init';",
                    "}",
                    "</script>",
                ]
                .join("\n")
                .to_string(),
            ),
        );
        // test1/components/MyComponent3.vue
        renderer.create_node_from_document(
            &TEST1_COMPONENT3,
            FullTextDocument::new(
                "vue".to_string(),
                0,
                [
                    "<template>",
                    "  <div></div>",
                    "</template>",
                    "<script lang=\"ts\">",
                    "import Vue from 'vue';",
                    "import { Component } from 'vue-property-decorator';",
                    "@Component",
                    "export default class MyComponent3 extends Vue {",
                    "  @Prop({ type: Boolean, default: false })",
                    "  private disabled!: boolean;",
                    "  private show = false;",
                    "}",
                    "</script>",
                ]
                .join("\n")
                .to_string(),
            ),
        );
        // test2/index.vue
        renderer.create_node_from_document(
            &TEST2_INDEX,
            FullTextDocument::new(
                "vue".to_string(),
                0,
                [
                    "<template>",
                    "  <MyComponent4 title=\"Title\" />",
                    "</template>",
                    "<script lang=\"ts\">",
                    "import Vue from 'vue';",
                    "import { Component } from 'vue-property-decorator';",
                    "import MyComponent4 from './ts/transfer.ts';",
                    "@Component({",
                    "  components: {",
                    "    MyComponent4,",
                    "  },",
                    "})",
                    "export default class Index extends Vue {",
                    "}",
                    "</script>",
                ]
                .join("\n")
                .to_string(),
            ),
        );
        // test2/ts/transfer.ts
        renderer.create_node_from_document(
            &TEST2_TS,
            FullTextDocument::new(
                "typescript".to_string(),
                0,
                [
                    "import MyComponent4 from '../components/MyComponent4.vue';",
                    "export default MyComponent4;",
                ]
                .join("\n")
                .to_string(),
            ),
        );
        // test2/components/MyComponent4.vue
        renderer.create_node_from_document(
            &TEST2_COMPONENT4,
            FullTextDocument::new(
                "vue".to_string(),
                0,
                [
                    "<template>",
                    "  <div></div>",
                    "</template>",
                    "<script lang=\"ts\">",
                    "import Vue from 'vue';",
                    "import { Component } from 'vue-property-decorator';",
                    "@Component",
                    "export default class MyComponent4 extends Vue {",
                    "  @Prop({ type: Boolean, default: false })",
                    "  private disabled!: boolean;",
                    "  private show = false;",
                    "}",
                    "</script>",
                ]
                .join("\n")
                .to_string(),
            ),
        );
        // test2/components/MyComponent5.vue
        renderer.create_node_from_document(
            &TEST2_COMPONENT5,
            FullTextDocument::new(
                "vue".to_string(),
                0,
                [
                    "<template>",
                    "  <div></div>",
                    "</template>",
                    "<script lang=\"ts\">",
                    "import Vue from 'vue';",
                    "import { Component } from 'vue-property-decorator';",
                    "@Component",
                    "export default class MyComponent5 extends Vue {",
                    "  @Prop({ type: Boolean, default: false })",
                    "  private disabled!: boolean;",
                    "  private show = false;",
                    "}",
                    "</script>",
                ]
                .join("\n")
                .to_string(),
            ),
        );
        renderer.render_cache.flush();

        renderer
    }

    fn create_empty_document() -> FullTextDocument {
        FullTextDocument::new("vue".to_string(), 0, "".to_string())
    }

    fn create_changes(
        changes: &[(u32, u32, u32, u32, Option<u32>, &str)],
    ) -> Vec<TextDocumentContentChangeEvent> {
        changes
            .iter()
            .map(|v| TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: v.0,
                        character: v.1,
                    },
                    end: Position {
                        line: v.2,
                        character: v.3,
                    },
                }),
                range_length: v.4,
                text: v.5.to_string(),
            })
            .collect()
    }

    fn create_params(
        uri: &Uri,
        changes: &[(u32, u32, u32, u32, Option<u32>, &str)],
    ) -> DidChangeTextDocumentParams {
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 1,
            },
            content_changes: create_changes(changes),
        }
    }

    fn assert_mapping(pos: (u32, u32), expected: Option<(u32, u32)>) {
        let renderer = create_renderer();
        let result = renderer.get_mapping_position(
            &TEST1_COMPONENT1,
            &Position {
                line: pos.0,
                character: pos.1,
            },
        );
        let expected = expected.map(|v| Position {
            line: v.0,
            character: v.1,
        });
        assert_eq!(result, expected);
    }

    fn assert_original(pos: (u32, u32), expected: Option<(u32, u32)>) {
        let renderer = create_renderer();
        let result = renderer.get_original_position(
            &TEST1_COMPONENT1,
            &Position {
                line: pos.0,
                character: pos.1,
            },
        );
        let expected = expected.map(|v| Position {
            line: v.0,
            character: v.1,
        });
        assert_eq!(result, expected);
    }

    fn assert_original_range(range: (u32, u32, u32, u32), expected: Option<(u32, u32, u32, u32)>) {
        let renderer = create_renderer();
        let result = renderer.get_original_range(
            &TEST1_COMPONENT1,
            &Range {
                start: Position {
                    line: range.0,
                    character: range.1,
                },
                end: Position {
                    line: range.2,
                    character: range.3,
                },
            },
        );
        let expected = expected.map(|v| Range {
            start: Position {
                line: v.0,
                character: v.1,
            },
            end: Position {
                line: v.2,
                character: v.3,
            },
        });
        assert_eq!(result, expected);
    }

    fn assert_position_type(pos: (u32, u32), expected: Option<PositionType>) {
        let renderer = create_renderer();
        let result = renderer.get_position_type(
            &TEST1_COMPONENT1,
            &Position {
                line: pos.0,
                character: pos.1,
            },
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn update_vue_script_props() {
        let mut renderer = create_renderer();
        let params = create_params(&TEST1_COMPONENT1, &[(9, 15, 9, 15, Some(0), "1")]);
        let expected = create_changes(&[
            (9, 15, 9, 15, Some(0), "1"),
            (11, 24, 11, 34, Some(10), "title1,text"),
        ]);
        let result = renderer.update(&TEST1_COMPONENT1, params, &create_empty_document());
        assert_eq!(result.content_changes, expected);
        // 上游节点应该更新
        assert_eq!(
            renderer
                .render_cache
                .get(&TEST1_INDEX)
                .unwrap()
                .get_version(),
            Some(1)
        );
    }

    #[test]
    fn update_vue_extends_relation() {
        let mut renderer = create_renderer();
        let extends_uri = renderer.render_cache.get_extends_uri(&TEST1_COMPONENT1);
        let expected_uri: Option<&Uri> = Some(&TEST1_COMPONENT2);
        assert_eq!(extends_uri, expected_uri);
        // 删除导入 MyComponent2
        let params = create_params(&TEST1_COMPONENT1, &[(5, 0, 5, 45, Some(45), "")]);
        let expected = create_changes(&[(5, 0, 5, 45, Some(45), "")]);
        let result = renderer.update(&TEST1_COMPONENT1, params, &create_empty_document());
        assert_eq!(result.content_changes, expected);
        let extends_uri = renderer.render_cache.get_extends_uri(&TEST1_COMPONENT1);
        assert_eq!(extends_uri, None);
        // 添加导入 MyComponent3
        let text = "import MyComponent3 from './MyComponent3.vue';";
        let params = create_params(&TEST1_COMPONENT1, &[(5, 0, 5, 0, Some(0), text)]);
        let expected = create_changes(&[(5, 0, 5, 0, Some(0), text)]);
        let result = renderer.update(&TEST1_COMPONENT1, params, &create_empty_document());
        assert_eq!(result.content_changes, expected);
        let extends_uri = renderer.render_cache.get_extends_uri(&TEST1_COMPONENT1);
        assert_eq!(extends_uri, None);
        // extends MyComponent2 改为 MyComponent3
        let params = create_params(&TEST1_COMPONENT1, &[(7, 53, 7, 54, Some(1), "3")]);
        let expected = create_changes(&[(7, 53, 7, 54, Some(1), "3")]);
        let result = renderer.update(&TEST1_COMPONENT1, params, &create_empty_document());
        assert_eq!(result.content_changes, expected);
        let extends_uri = renderer.render_cache.get_extends_uri(&TEST1_COMPONENT1);
        let expected_uri: Option<&Uri> = Some(&TEST1_COMPONENT3);
        assert_eq!(extends_uri, expected_uri);
    }

    #[test]
    fn update_vue_registers_relation() {
        let mut renderer = create_renderer();
        // 将 ./components/MyComponent1.vue 替换为 ./components/MyComponent2.vue
        let params = create_params(&TEST1_INDEX, &[(6, 50, 6, 51, Some(1), "2")]);
        let expected = create_changes(&[(6, 50, 6, 51, Some(1), "2")]);
        let result = renderer.update(&TEST1_INDEX, params, &create_empty_document());
        assert_eq!(result.content_changes, expected);
        let registers = renderer.render_cache.get_registers(&TEST1_INDEX);
        let expected_uri: &Uri = &TEST1_COMPONENT2;
        assert_eq!(
            registers,
            vec![("MyComponent1".to_string(), None, None, expected_uri,)]
        );
        // 将 import MyComponent1 改为 import MyComponent2
        let params = create_params(&TEST1_INDEX, &[(6, 18, 6, 19, Some(1), "2")]);
        let expected = create_changes(&[(6, 18, 6, 19, Some(1), "2")]);
        let result = renderer.update(&TEST1_INDEX, params, &create_empty_document());
        assert_eq!(result.content_changes, expected);
        let registers = renderer.render_cache.get_registers(&TEST1_INDEX);
        assert_eq!(registers, vec![]);
        // 将 components 内的 MyComponent1 改为 MyComponent2
        let params = create_params(&TEST1_INDEX, &[(9, 15, 9, 16, Some(1), "2")]);
        let expected = create_changes(&[(9, 15, 9, 16, Some(1), "2")]);
        let result = renderer.update(&TEST1_INDEX, params, &create_empty_document());
        assert_eq!(result.content_changes, expected);
        let registers = renderer.render_cache.get_registers(&TEST1_INDEX);
        let expected_uri: &Uri = &TEST1_COMPONENT2;
        assert_eq!(
            registers,
            vec![("MyComponent2".to_string(), None, None, expected_uri,)]
        );
    }

    #[test]
    fn update_ts_transfers() {
        let mut renderer = create_renderer();
        let transfer_result = renderer.render_cache.get_transfer_node(&TEST2_TS, &None);
        let expected: Option<(&Uri, Option<_>)> = Some((&TEST2_COMPONENT4, None));
        assert_eq!(transfer_result, expected);
        // ../components/MyComponent4.vue 改为 ../components/MyComponent5.vue
        let params = create_params(&TEST2_TS, &[(0, 51, 0, 52, Some(1), "5")]);
        let expected = create_changes(&[(0, 51, 0, 52, Some(1), "5")]);
        let result = renderer.update(&TEST2_TS, params, &create_empty_document());
        assert_eq!(result.content_changes, expected);
        let transfer_result = renderer.render_cache.get_transfer_node(&TEST2_TS, &None);
        let expected: Option<(&Uri, Option<_>)> = Some((&TEST2_COMPONENT5, None));
        assert_eq!(transfer_result, expected);
    }

    #[test]
    fn update_full() {
        let mut renderer = create_renderer();
        let params = create_params(&TEST1_INDEX, &[(3, 7, 3, 7, Some(0), "1")]);
        let expected = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: TEST1_INDEX.clone(),
                version: 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "".to_string(),
            }],
        };
        let result = renderer.update(
            &TEST1_INDEX,
            params,
            &FullTextDocument::new(
                "vue".to_string(),
                1,
                [
                    "<template>",
                    "  <MyComponent1 title=\"Title\" />",
                    "</template>",
                    "<script1 lang=\"ts\">",
                    "import Vue from 'vue';",
                    "import { Component } from 'vue-property-decorator';",
                    "import MyComponent1 from './components/MyComponent1.vue';",
                    "@Component({",
                    "  components: {",
                    "    MyComponent1,",
                    "  },",
                    "})",
                    "export default class Index extends Vue {",
                    "}",
                    "</script>",
                ]
                .join("\n"),
            ),
        );
        assert_eq!(result, expected);

        let params = create_params(&TEST1_INDEX, &[(3, 7, 3, 8, Some(1), "")]);
        let result = renderer.update(
            &TEST1_INDEX,
            params,
            &FullTextDocument::new(
                "vue".to_string(),
                1,
                [
                    "<template>",
                    "  <MyComponent1 title=\"Title\" />",
                    "</template>",
                    "<script lang=\"ts\">",
                    "import Vue from 'vue';",
                    "import { Component } from 'vue-property-decorator';",
                    "import MyComponent1 from './components/MyComponent1.vue';",
                    "@Component({",
                    "  components: {",
                    "    MyComponent1,",
                    "  },",
                    "})",
                    "export default class Index extends Vue {",
                    "}",
                    "</script>",
                ]
                .join("\n"),
            ),
        );
        let expected = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: TEST1_INDEX.clone(),
                version: 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: renderer
                    .render_cache
                    .get_node_render_content(&TEST1_INDEX)
                    .unwrap(),
            }],
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn mapping() {
        assert_mapping((1, 14), None);
        assert_mapping((1, 15), Some((12, 1)));
        assert_mapping((1, 19), Some((12, 5)));
        assert_mapping((1, 20), Some((12, 6)));
        assert_mapping((1, 32), None);
        assert_mapping((1, 33), Some((12, 9)));
        assert_mapping((1, 34), None);
        assert_mapping((1, 36), None);
        assert_mapping((1, 37), Some((12, 12)));
        assert_mapping((1, 42), Some((12, 17)));
        assert_mapping((1, 43), Some((12, 18)));
        assert_mapping((1, 44), None);
    }

    #[test]
    fn mapping_ts() {
        let renderer = create_renderer();
        let result = renderer.get_mapping_position(
            &TEST2_TS,
            &Position {
                line: 0,
                character: 0,
            },
        );
        assert_eq!(result, None);
    }

    #[test]
    fn original() {
        assert_original((12, 0), None);
        assert_original((12, 1), Some((1, 15)));
        assert_original((12, 5), Some((1, 19)));
        assert_original((12, 6), Some((1, 20)));
        assert_original((12, 7), None);
        assert_original((12, 8), None);
        assert_original((12, 9), Some((1, 33)));
        assert_original((12, 10), None);
        assert_original((12, 11), None);
        assert_original((12, 12), Some((1, 37)));
        assert_original((12, 17), Some((1, 42)));
        assert_original((12, 18), Some((1, 43)));
        assert_original((12, 19), None);
        assert_original((11, 0), None);
    }

    #[test]
    fn original_ts() {
        let renderer = create_renderer();
        let result = renderer.get_original_position(
            &TEST2_TS,
            &Position {
                line: 0,
                character: 0,
            },
        );
        assert_eq!(result, None);
    }

    #[test]
    fn original_range() {
        assert_original_range((12, 1, 12, 6), Some((1, 15, 1, 20)));
        assert_original_range((12, 1, 12, 7), None);
        assert_original_range((12, 0, 12, 6), None);
        assert_original_range((12, 9, 12, 9), Some((1, 33, 1, 33)));
    }

    #[test]
    fn position_type() {
        assert_position_type((0, 0), None);
        assert_position_type((1, 3), Some(PositionType::Template));
        assert_position_type((1, 14), Some(PositionType::Template));
        assert_position_type(
            (1, 15),
            Some(PositionType::TemplateExpr(Position {
                line: 12,
                character: 1,
            })),
        );
        assert_position_type((3, 0), None);
        assert_position_type((4, 0), Some(PositionType::Script));
        assert_position_type((12, 0), None);
    }
}
