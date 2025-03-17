use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;

use html_languageservice::html_data::{IAttributeData, ITagData, IValueData};
use html_languageservice::language_facts::data_provider::{
    generate_documentation, GenerateDocumentationItem, GenerateDocumentationSetting,
    IHTMLDataProvider,
};
use html_languageservice::participant::{
    HtmlAttributeValueContext, HtmlContentContext, ICompletionParticipant,
};
use regex::Regex;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Documentation, InsertTextFormat,
    Position, Range, TextEdit, Url,
};

use super::render_cache::RenderCache;
use super::Renderer;

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
                        description: cache.description.clone(),
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
                            description: ts_component.description.clone(),
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
                            description: component.description.clone(),
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

    fn get_document_version(&self, uri: &Url) -> Option<i32> {
        let cache = &self.render_cache[uri];
        if let RenderCache::VueRenderCache(cache) = cache {
            Some(cache.document.version())
        } else {
            None
        }
    }
}

pub struct ArcTagsProvider(Arc<TagsProvider>);

impl ArcTagsProvider {
    pub fn new(id: String, tags: Vec<ITagData>, version: Option<i32>) -> Self {
        ArcTagsProvider(Arc::new(TagsProvider::new(id, tags, version)))
    }
}

impl Deref for ArcTagsProvider {
    type Target = TagsProvider;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for ArcTagsProvider {
    fn clone(&self) -> Self {
        ArcTagsProvider(Arc::clone(&self.0))
    }
}

#[derive(Clone)]
pub struct TagsProvider {
    id: String,
    tags: Vec<ITagData>,
    version: Option<i32>,
}

impl TagsProvider {
    pub fn new(id: String, tags: Vec<ITagData>, version: Option<i32>) -> TagsProvider {
        TagsProvider { id, version, tags }
    }

    pub fn version(&self) -> Option<i32> {
        return self.version;
    }
}

impl Debug for TagsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TagsProvider { Unknown }").finish()
    }
}

impl IHTMLDataProvider for ArcTagsProvider {
    fn get_id(&self) -> &str {
        &self.id
    }

    fn is_applicable(&self, _language_id: &str) -> bool {
        true
    }

    fn provide_tags(&self) -> &Vec<ITagData> {
        &self.tags
    }

    fn provide_attributes(&self, tag: &str) -> Vec<&IAttributeData> {
        let tag_data = self.tags.iter().find(|t| t.name == tag);
        if let Some(tag_data) = tag_data {
            tag_data.attributes.iter().collect()
        } else {
            vec![]
        }
    }

    fn provide_values(&self, _tag: &str, _attribute: &str) -> Vec<&IValueData> {
        vec![]
    }
}

#[tower_lsp::async_trait]
impl ICompletionParticipant for ArcTagsProvider {
    async fn on_html_attribute_value(
        &self,
        _context: HtmlAttributeValueContext,
    ) -> Vec<CompletionItem> {
        vec![]
    }

    async fn on_html_content(&self, context: HtmlContentContext) -> Vec<CompletionItem> {
        let document = context.document;
        let html_document = context.html_document;
        let position = context.position;
        let offset = document.offset_at(position);
        if let Some(root) = html_document.find_root_at(offset as usize) {
            if root.tag.as_ref().is_some_and(|tag| tag == "template") {
                let range = Range {
                    start: Position {
                        line: position.line,
                        character: 0,
                    },
                    end: Position {
                        line: position.line + 1,
                        character: 0,
                    },
                };
                let text = document.get_content(Some(range));
                let text_length = text.len() - 1;
                let trim_length = text.len() - text.trim_start().len();
                if Regex::new(r"^\s*([A-Z][a-zA-Z0-9_]*)\s$")
                    .unwrap()
                    .is_match(text)
                {
                    return (&self.tags)
                        .iter()
                        .map(|tag| {
                            let documentation = generate_documentation(
                                GenerateDocumentationItem {
                                    description: tag.description.clone(),
                                    references: tag.references.clone(),
                                },
                                GenerateDocumentationSetting {
                                    documentation: true,
                                    references: true,
                                    does_support_markdown: true,
                                },
                            );
                            let documentation = if let Some(documentation) = documentation {
                                Some(Documentation::MarkupContent(documentation))
                            } else {
                                None
                            };
                            CompletionItem {
                                label: tag.name.clone(),
                                kind: Some(CompletionItemKind::PROPERTY),
                                documentation,
                                text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                                    Range {
                                        start: Position {
                                            line: position.line,
                                            character: 0,
                                        },
                                        end: Position {
                                            line: position.line,
                                            character: text_length as u32,
                                        },
                                    },
                                    format!(
                                        "{}<{}$0></{}>",
                                        &text[..trim_length],
                                        &tag.name,
                                        &tag.name
                                    ),
                                ))),
                                insert_text_format: Some(InsertTextFormat::SNIPPET),
                                ..Default::default()
                            }
                        })
                        .collect::<Vec<_>>();
                }
            }
        }
        vec![]
    }
}
