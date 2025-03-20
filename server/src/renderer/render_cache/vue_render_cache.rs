use html_languageservice::{html_data::Description, parser::html_document::Node};
use lsp_textdocument::FullTextDocument;
use swc_common::source_map::SmallPos;
use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

use crate::{
    lazy::REG_SINGLE_BRACKET,
    renderer::{
        parse_document,
        parse_script::{self, ExtendsComponent, ParseScriptResult, RegisterComponent},
        template_compile::{self, CompileMapping},
    },
};

use super::RenderCacheUpdateResult;

/// vue 组件的渲染缓存
#[derive(Debug)]
pub struct VueRenderCache {
    /// 渲染前的文档，与文件系统中相同
    pub document: FullTextDocument,
    // 解析文档
    pub template: Node,
    pub script: Node,
    pub style: Vec<Node>,
    // 解析模版
    pub name_range: (usize, usize),
    pub description: Option<Description>,
    pub template_compile_result: String,
    pub mapping: CompileMapping,
    /// 解析脚本得到的属性
    pub props: Vec<String>,
    pub render_insert_offset: usize,
    /// 安全更新范围，处于此范围的更新无需重新解析脚本
    pub safe_update_range: Vec<(usize, usize)>,
}

impl VueRenderCache {
    /// 更新，如果更新失败需要重新解析，那么返回 None
    pub fn update(
        &mut self,
        change: TextDocumentContentChangeEvent,
        document: &FullTextDocument,
    ) -> Option<RenderCacheUpdateResult> {
        let range = change.range.unwrap();
        let range_length = change.range_length.unwrap() as usize;
        let range_start = self.document.offset_at(range.start) as usize;
        let range_end = self.document.offset_at(range.end) as usize;
        // 如果变更处于 style，那么 range 位置向下移动一行
        let style_range = Range {
            start: Position {
                line: range.start.line + 1,
                character: range.start.character,
            },
            end: Position {
                line: range.end.line + 1,
                character: range.end.character,
            },
        };
        // 更新缓存文档
        self.document.update(&[change.clone()], document.version());
        let source = document.get_content(None);
        // 节点需要增加的偏移量
        let incremental = change.text.len() as isize - range_length as isize;
        // 1. 如果变更处于 template 节点
        if self.template.start < range_start && range_end < self.template.end {
            // 位移
            self.move_offset(range_start, incremental);
            // 重新解析 template 节点
            let node = parse_document::parse_as_node(
                document,
                Some(Range::new(
                    document.position_at(self.template.start as u32),
                    document.position_at(self.template.end as u32),
                )),
            );

            if let Some(node) = node {
                self.template = node;
                // 进行模版编译
                let (template_compile_result, mapping) =
                    template_compile::template_compile(&self.template, source);
                let old_template_compile_result = self.template_compile_result.clone();
                self.template_compile_result = template_compile_result;
                self.mapping = mapping;
                // template_compile_result 插入的行
                let line = self
                    .document
                    .position_at(self.render_insert_offset as u32 + 1)
                    .line
                    + 1; // template_compile_result 前有换行
                return Some(RenderCacheUpdateResult {
                    changes: vec![
                        // 模版对应位置填充空格
                        TextDocumentContentChangeEvent {
                            range: change.range,
                            range_length: change.range_length,
                            text: " ".repeat(change.text.len()),
                        },
                        // 替换 template_compile_result
                        TextDocumentContentChangeEvent {
                            range: Some(Range {
                                start: Position { line, character: 0 },
                                end: Position {
                                    line,
                                    character: old_template_compile_result.len() as u32,
                                },
                            }),
                            range_length: Some(old_template_compile_result.len() as u32),
                            text: self.template_compile_result.clone(),
                        },
                    ],
                    is_change_prop: false,
                    extends_component: None,
                    registers: None,
                    transfers: None,
                });
            } else {
                self.template.end = (self.template.end as isize + incremental) as usize;
                // template 节点解析失败，将变更内容转换为空格后输出
                return Some(RenderCacheUpdateResult {
                    changes: vec![
                        // 模版对应位置填充空格
                        TextDocumentContentChangeEvent {
                            range: change.range,
                            range_length: change.range_length,
                            text: " ".repeat(change.text.len()),
                        },
                    ],
                    is_change_prop: false,
                    extends_component: None,
                    registers: None,
                    transfers: None,
                });
            }
        }
        // 2. 如果变更处于 script 节点
        if self.script.start_tag_end.is_some_and(|v| v <= range_start)
            && self.script.end_tag_start.is_some_and(|v| range_end < v)
        {
            // 如果可以安全更新，那么直接修改 render_insert_offset 后返回
            if is_safe_update(
                range_start,
                range_end,
                &mut self.safe_update_range,
                &change.text,
            ) {
                self.move_offset(range_start, incremental);
                return Some(RenderCacheUpdateResult {
                    changes: vec![change],
                    is_change_prop: false,
                    extends_component: None,
                    registers: None,
                    transfers: None,
                });
            } else {
                self.move_offset(range_start, incremental);
                if let Some(ParseScriptResult {
                    name_span,
                    description,
                    props,
                    render_insert_offset,
                    extends_component,
                    registers,
                    safe_update_range,
                }) = parse_script::parse_script(
                    source,
                    self.script.start_tag_end.unwrap(),
                    self.script.end_tag_start.unwrap(),
                ) {
                    // 尝试`解析脚本` 成功
                    self.render_insert_offset = render_insert_offset;
                    self.name_range = (name_span.lo.to_usize(), name_span.hi.to_usize());
                    self.description = description;

                    let is_change_prop = self.props != props;
                    let old_props_length = self.props.join(",").len() as u32;
                    self.props = props;

                    self.safe_update_range = safe_update_range;
                    let Position { line, character } = self
                        .document
                        .position_at(self.render_insert_offset as u32 + 1);
                    return Some(RenderCacheUpdateResult {
                        changes: vec![
                            change,
                            // 属性变更
                            TextDocumentContentChangeEvent {
                                range: Some(Range {
                                    start: Position {
                                        line,
                                        character: character + 23,
                                    },
                                    end: Position {
                                        line,
                                        character: character + 23 + old_props_length,
                                    },
                                }),
                                range_length: Some(old_props_length),
                                text: self.props.join(","),
                            },
                        ],
                        is_change_prop,
                        extends_component,
                        registers: Some(registers),
                        transfers: None,
                    });
                } else {
                    // 解析失败
                    self.safe_update_range = vec![];
                    return Some(RenderCacheUpdateResult {
                        changes: vec![change],
                        is_change_prop: false,
                        extends_component: None,
                        registers: None,
                        transfers: None,
                    });
                }
            }
        }

        // 3. 如果变更位于 style 节点
        let mut is_in_style = false;
        for style in &self.style {
            if range_start > style.start && range_start < style.end {
                is_in_style = true;
                break;
            }
        }
        if is_in_style {
            // 变更处于 style，将变更转换为空格后输出
            self.move_offset(range_start, incremental);
            return Some(RenderCacheUpdateResult {
                changes: vec![
                    // 模版对应位置填充空格
                    TextDocumentContentChangeEvent {
                        range: Some(style_range),
                        range_length: change.range_length,
                        text: " ".repeat(change.text.len()),
                    },
                ],
                is_change_prop: false,
                extends_component: None,
                registers: None,
                transfers: None,
            });
        }

        // 4. 如果变更处于节点边界，返回 None 进行全量渲染
        None
    }

    /// 从 offset 之后的位置开始移动 incremental，为正向后移动，为负向前移动
    /// 要求 document 已更新
    fn move_offset(&mut self, offset: usize, incremental: isize) {
        fn move_it(v: &mut usize, incremental: isize) {
            *v = (*v as isize + incremental) as usize;
        }
        fn move_node(node: &mut Node, offset: usize, incremental: isize) {
            if offset >= node.end {
                return;
            }
            move_it(&mut node.end, incremental);
            if offset < node.start {
                move_it(&mut node.start, incremental);
            }
            if node.start_tag_end.is_some() && offset < node.start_tag_end.unwrap() {
                move_it(node.start_tag_end.as_mut().unwrap(), incremental);
                for attr in node.attributes.values_mut() {
                    if offset < attr.offset {
                        move_it(&mut attr.offset, incremental);
                    }
                }
            }
            if node.end_tag_start.is_some() && offset < node.end_tag_start.unwrap() {
                move_it(node.end_tag_start.as_mut().unwrap(), incremental);
                for child in &mut node.children {
                    move_node(child, offset, incremental);
                }
            }
        }
        // 移动 template
        move_node(&mut self.template, offset, incremental);
        // 移动 script
        move_node(&mut self.script, offset, incremental);
        // 移动 style
        for style in &mut self.style {
            move_node(style, offset, incremental);
        }
        // 移动 name_range
        if offset < self.name_range.0 {
            move_it(&mut self.name_range.0, incremental);
        }
        if offset < self.name_range.1 {
            move_it(&mut self.name_range.1, incremental);
        }
        // 移动 mapping
        for item in &mut self.mapping {
            if offset < item.1 {
                move_it(&mut item.1, incremental);
            }
        }
        // 移动 render_insert_offset
        if offset < self.render_insert_offset {
            move_it(&mut self.render_insert_offset, incremental);
        }
        // 移动 safe_update_range
        for item in &mut self.safe_update_range {
            if offset < item.0 {
                move_it(&mut item.0, incremental);
            }
            if offset < item.1 {
                move_it(&mut item.1, incremental);
            }
        }
    }
}

/// 解析 vue 组件
pub fn parse_vue_file(document: &FullTextDocument) -> Option<ParseVueFileResult> {
    // 解析文档
    let (template, script, style) = parse_document::parse_document(&document);

    let template = template?;
    let script = script?;
    let source = document.get_content(None);
    // 解析脚本
    let ParseScriptResult {
        name_span,
        description,
        props,
        render_insert_offset,
        extends_component,
        registers,
        safe_update_range,
    } = parse_script::parse_script(
        source,
        script.start_tag_end.unwrap(),
        script.end_tag_start.unwrap(),
    )?;
    // 模版编译
    let (template_compile_result, mapping) = template_compile::template_compile(&template, source);

    Some(ParseVueFileResult {
        template,
        script,
        style,
        name_range: (name_span.lo.to_usize(), name_span.hi.to_usize()),
        description,
        props,
        render_insert_offset,
        template_compile_result,
        mapping,
        extends_component,
        registers,
        safe_update_range,
    })
}

pub struct ParseVueFileResult {
    pub template: Node,
    pub script: Node,
    pub style: Vec<Node>,
    pub name_range: (usize, usize),
    pub description: Option<Description>,
    /// 渲染得到的属性
    pub props: Vec<String>,
    pub render_insert_offset: usize,
    pub template_compile_result: String,
    pub mapping: CompileMapping,
    pub extends_component: Option<ExtendsComponent>,
    pub registers: Vec<RegisterComponent>,
    pub safe_update_range: Vec<(usize, usize)>,
}

/// 是否可以安全更新
/// * 如果变更包含单独大括号，那么需要重新解析脚本
fn is_safe_update(
    range_start: usize,
    range_end: usize,
    safe_update_range: &mut Vec<(usize, usize)>,
    text: &str,
) -> bool {
    // 如果 text 包含单独的大括号，那么返回 false
    if REG_SINGLE_BRACKET.is_match(text) {
        return false;
    }
    if safe_update_range.len() == 0 {
        return false;
    }
    let mut min = 0;
    let mut max = safe_update_range.len() - 1;
    let mut mid = (min + max) / 2;
    while min < max {
        let (left, right) = safe_update_range[mid];
        if range_start > right {
            if min == mid {
                min += 1;
            } else {
                min = mid;
            }
        } else if range_start < left {
            max = mid;
        } else {
            return range_end < right;
        }
        mid = (min + max) / 2;
    }
    false
}

#[cfg(test)]
mod tests {
    use lsp_textdocument::FullTextDocument;
    use swc_common::source_map::SmallPos;
    use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

    use crate::renderer::{
        combined_rendered_results, parse_document, parse_script, template_compile,
    };

    use super::VueRenderCache;

    fn assert_update(changes: &[TextDocumentContentChangeEvent]) {
        let mut document = FullTextDocument::new(
            "vue".to_string(),
            0,
            [
                // 0    5   10   15   20   25   30   35   40   45   50   55   60
                r#"<template>"#,
                r#"  <div>"#,
                r#"    <MyComponent1 :title="title"></MyComponent1>"#, // value expr
                r#"    <div>{{ content }}</div>"#,                     // content expr
                r#"    <MyComponent2 v-if="condition1" />"#,           // condition expr
                r#"    <Empty v-else :description="'text'" />"#,
                r#"  </div>"#,
                r#"</template>"#,
                r#"<script lang="ts">"#,
                r#"import Vue from "vue";"#,
                r#"import { Component, Model, Prop } from "vue-property-decorator";"#,
                r#"import { Empty } from "ant-design-vue";"#,
                r#"import MyComponent1 from "./my-component1.vue";"#,
                r#"import MyComponent2 from "./my-component2.vue";"#,
                r#"@Component({"#,
                r#"  components: {"#,
                r#"    Empty,"#,
                r#"    MyComponent1,"#,
                r#"    MyComponent2,"#,
                r#"  },"#,
                r#"})"#,
                r#"export default class App extends Vue {"#,
                r#"  @Model("change", { type: Number, required: true })"#,
                r#"  private value!: number;"#,
                r#"  @Prop({ type: String, default: "" })"#,
                r#"  private prop1!: string;"#,
                r#"  private prop2 = "";"#,
                r#"  public prop3 = "";"#,
                r#"  private get prop4() {"#,
                r#"    return this.value === 1;"#,
                r#"  }"#,
                r#"  created() {"#,
                r#"    this.calc();"#,
                r#"  }"#,
                r#"  private calc() {"#,
                r#"    123 + 456;"#,
                r#"  }"#,
                r#"}"#,
                r#"</script>"#,
                r#"<style>"#,
                r#".root {"#,
                r#"  display: flex;"#,
                r#"}"#,
                r#"</style>"#,
            ]
            .join("\n"),
        );
        let mut cache = create_vue_render_cache(&document);
        let mut old_render_result = get_render_content(&cache);

        // update
        for (i, change) in changes.iter().enumerate() {
            document.update(&[change.clone()], i as i32 + 1);
            let render_changes = cache.update(change.clone(), &document).unwrap().changes;
            let render_result = get_render_content(&cache);

            // old_render_result + changes = render_result
            let mut render_document =
                FullTextDocument::new("typescript".to_string(), 0, old_render_result.clone());
            render_document.update(&render_changes, 1);
            assert_eq!(render_document.get_content(None), render_result);
            old_render_result = render_result;
        }

        let expected = create_vue_render_cache(&document);
        assert_eq!(
            cache.document.get_content(None),
            expected.document.get_content(None)
        );
        assert_eq!(cache.template, expected.template);
        assert_eq!(cache.script, expected.script);
        assert_eq!(cache.style, expected.style);
        assert_eq!(cache.name_range, expected.name_range);
        assert_eq!(cache.description, expected.description);
        assert_eq!(
            cache.template_compile_result,
            expected.template_compile_result
        );
        assert_eq!(cache.mapping, expected.mapping);
        assert_eq!(cache.props, expected.props);
        assert_eq!(cache.render_insert_offset, expected.render_insert_offset);
        assert_eq!(cache.safe_update_range, expected.safe_update_range);
    }

    fn create_vue_render_cache(document: &FullTextDocument) -> VueRenderCache {
        let source = document.get_content(None);
        let (template, script, style) = parse_document::parse_document(&document);
        let template = template.unwrap();
        let script = script.unwrap();
        let result = parse_script::parse_script(
            source,
            script.start_tag_end.unwrap(),
            script.end_tag_start.unwrap(),
        )
        .unwrap();
        let (template_compile_result, mapping) =
            template_compile::template_compile(&template, source);
        VueRenderCache {
            document: FullTextDocument::new(
                document.language_id().to_string(),
                document.version(),
                source.to_string(),
            ),
            template,
            script,
            style,
            name_range: (
                result.name_span.lo.to_usize(),
                result.name_span.hi.to_usize(),
            ),
            description: result.description,
            template_compile_result,
            mapping,
            props: result.props,
            render_insert_offset: result.render_insert_offset,
            safe_update_range: result.safe_update_range,
        }
    }

    fn get_render_content(cache: &VueRenderCache) -> String {
        combined_rendered_results::combined_rendered_results(
            cache.script.start_tag_end.unwrap(),
            cache.script.end_tag_start.unwrap(),
            &cache.template_compile_result,
            &cache.props,
            cache.render_insert_offset,
            cache.document.get_content(None),
        )
    }

    #[test]
    fn template_tag_name_update() {
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 2,
                    character: 16,
                },
                end: Position {
                    line: 2,
                    character: 17,
                },
            }),
            range_length: Some(1),
            text: "2".to_string(),
        }]);
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 2,
                    character: 17,
                },
                end: Position {
                    line: 2,
                    character: 17,
                },
            }),
            range_length: Some(0),
            text: "2".to_string(),
        }]);
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 2,
                    character: 16,
                },
                end: Position {
                    line: 2,
                    character: 17,
                },
            }),
            range_length: Some(1),
            text: "".to_string(),
        }]);
    }

    #[test]
    fn template_complete_update() {
        // 删除 title 属性和它的值
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 2,
                    character: 17,
                },
                end: Position {
                    line: 2,
                    character: 32,
                },
            }),
            range_length: Some(15),
            text: "".to_string(),
        }]);
    }

    #[test]
    fn template_fail_update() {
        assert_update(&[
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 2,
                        character: 33,
                    },
                    end: Position {
                        line: 2,
                        character: 40,
                    },
                }),
                range_length: Some(7),
                text: "".to_string(),
            },
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 2,
                        character: 33,
                    },
                    end: Position {
                        line: 2,
                        character: 33,
                    },
                }),
                range_length: Some(0),
                text: "</MyCom".to_string(),
            },
        ]);
    }

    #[test]
    fn script_safe_update() {
        // 删除 prop4 中的 "value" 不包含 e
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 29,
                    character: 16,
                },
                end: Position {
                    line: 29,
                    character: 20,
                },
            }),
            range_length: Some(4),
            text: "".to_string(),
        }]);
        // 删除 prop4 中的 "value" 然后添加回来
        assert_update(&[
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 29,
                        character: 16,
                    },
                    end: Position {
                        line: 29,
                        character: 21,
                    },
                }),
                range_length: Some(5),
                text: "".to_string(),
            },
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 29,
                        character: 16,
                    },
                    end: Position {
                        line: 29,
                        character: 16,
                    },
                }),
                range_length: Some(0),
                text: "value".to_string(),
            },
        ]);
        // 删除 created 中的 "calc"，不包含最后一个 c
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 32,
                    character: 10,
                },
                end: Position {
                    line: 32,
                    character: 13,
                },
            }),
            range_length: Some(3),
            text: "".to_string(),
        }]);
        // 删除 created 中的 "calc" ，然后添加回来
        assert_update(&[
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 32,
                        character: 9,
                    },
                    end: Position {
                        line: 32,
                        character: 13,
                    },
                }),
                range_length: Some(4),
                text: "".to_string(),
            },
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 32,
                        character: 9,
                    },
                    end: Position {
                        line: 32,
                        character: 9,
                    },
                }),
                range_length: Some(0),
                text: "calc".to_string(),
            },
        ]);
        // 删除 calc 中的 "45"
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 35,
                    character: 11,
                },
                end: Position {
                    line: 35,
                    character: 13,
                },
            }),
            range_length: Some(2),
            text: "".to_string(),
        }]);
        // 删除 calc 中的 "456"，然后添加回来
        assert_update(&[
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 35,
                        character: 11,
                    },
                    end: Position {
                        line: 35,
                        character: 14,
                    },
                }),
                range_length: Some(3),
                text: "".to_string(),
            },
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 35,
                        character: 11,
                    },
                    end: Position {
                        line: 35,
                        character: 11,
                    },
                }),
                range_length: Some(0),
                text: "456".to_string(),
            },
        ]);
    }

    #[test]
    fn script_parse_script_complete_update() {
        // 跨越安全更新范围
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 32,
                    character: 0,
                },
                end: Position {
                    line: 35,
                    character: 0,
                },
            }),
            range_length: Some(40),
            text: "".to_string(),
        }]);
        // 处于安全更新范围外
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 31,
                    character: 0,
                },
                end: Position {
                    line: 31,
                    character: 0,
                },
            }),
            range_length: Some(0),
            text: "private prop5 = \"\";\n".to_string(),
        }]);
    }

    #[test]
    fn script_parse_script_fail_update() {
        // 先删除 "calc(" 然后恢复
        assert_update(&[
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 34,
                        character: 10,
                    },
                    end: Position {
                        line: 34,
                        character: 15,
                    },
                }),
                range_length: Some(5),
                text: "".to_string(),
            },
            TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 34,
                        character: 10,
                    },
                    end: Position {
                        line: 34,
                        character: 10,
                    },
                }),
                range_length: Some(0),
                text: "calc(".to_string(),
            },
        ]);
    }

    #[test]
    fn style_update() {
        assert_update(&[TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 40,
                    character: 5,
                },
                end: Position {
                    line: 40,
                    character: 5,
                },
            }),
            range_length: Some(0),
            text: "-container".to_string(),
        }]);
    }
}
