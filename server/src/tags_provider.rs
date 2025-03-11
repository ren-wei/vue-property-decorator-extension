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
    Position, Range, TextEdit,
};

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
