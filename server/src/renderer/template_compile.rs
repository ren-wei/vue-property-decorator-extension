use html_languageservice::parser::html_document::Node;
use multi_line_stream::MultiLineStream;

use crate::lazy::{REG_DOUBLE_BRACES, REG_V_FOR_WITH_INDEX};

/// 模版编译，返回 template_compile_result, mapping
pub fn template_compile(template: &Node, source: &str) -> (String, CompileMapping) {
    let mut result = TemplateCompileResult::new();
    compile_node(template, source, &mut result);
    (result.render, result.mapping)
}

fn compile_node(node: &Node, source: &str, result: &mut TemplateCompileResult) {
    let mut close_str = "";

    let attrs = node.attribute_names();

    // v-if
    let mut skip_util_v_if = false;
    let v_if_key = "v-if";
    if attrs.iter().find(|v| **v == v_if_key).is_some() {
        let value = node.attributes.get(v_if_key).unwrap();
        let value_offset = value.offset + v_if_key.len() + 2;
        if let Some(value) = &value.value {
            if value.starts_with(r#"""#) && value.ends_with(r#"""#) && value.len() > 1 {
                result.add_wrap("if(");
                result.add_fragment(&value[1..value.len() - 1], value_offset);
                result.add_wrap("){");
                close_str = "}";
                skip_util_v_if = true;
            }
        }
    }

    // v-else-if
    let mut skip_util_v_else_if = false;
    let v_else_if_key = "v-else-if";
    if attrs.iter().find(|v| **v == v_else_if_key).is_some() {
        let value = node.attributes.get(v_else_if_key).unwrap();
        let value_offset = value.offset + v_else_if_key.len() + 2;
        if let Some(value) = &value.value {
            if value.starts_with(r#"""#) && value.ends_with(r#"""#) && value.len() > 1 {
                result.add_wrap("else if(");
                result.add_fragment(&value[1..value.len() - 1], value_offset);
                result.add_wrap("){");
                close_str = "}";
                skip_util_v_else_if = true;
            }
        }
    }

    // v-else
    let v_else_key = "v-else";
    if attrs.iter().find(|v| **v == v_else_key).is_some() {
        result.add_wrap("else{");
        close_str = "}";
    }

    // v-for
    let v_for_key = "v-for";
    if attrs.iter().find(|v| **v == v_for_key).is_some() {
        let value = node.attributes.get(v_for_key).unwrap();
        if let Some(value) = &value.value {
            if value.starts_with(r#"""#) && value.ends_with(r#"""#) && value.len() > 1 {
                let value = &value[1..value.len() - 1];
                if let Some((left, right)) = value.split_once(" in ") {
                    if REG_V_FOR_WITH_INDEX.is_match(left) {
                        result.add_wrap(&format!("for(const __item__ of {right})"));
                        result.add_wrap("{");
                        close_str = "}"
                    } else {
                        result.add_wrap(&format!("for(const __item__ of {right})"));
                        result.add_wrap("{");
                        close_str = "}";
                    }
                }
            }
        }
    }

    for key in node.attribute_names_by_order() {
        let value = node.attributes.get(key).unwrap();
        if key.starts_with(":")
            || key.starts_with("@")
            || key.starts_with("#")
            || (key.starts_with("v-") && key != v_if_key && key != v_else_if_key)
            || ["slot-scope"].contains(&&key[..])
        {
            let value_offset = value.offset + key.len() + 2;
            if let Some(value) = &value.value {
                if value.starts_with(r#"""#) && value.ends_with(r#"""#) && value.len() > 1 {
                    let value = &value[1..value.len() - 1];
                    if key == v_for_key {
                        if let Some((left, right)) = value.split_once(" in ") {
                            if let Some(caps) = REG_V_FOR_WITH_INDEX.captures(left) {
                                let item = caps.get(1).unwrap();
                                let index = caps.get(2).unwrap();
                                result.add_wrap("const ");
                                result.add_fragment(item.as_str(), value_offset + item.start());
                                result.add_wrap(" = __item__;");
                                result.add_wrap("const ");
                                result.add_fragment(index.as_str(), value_offset + index.start());
                                result.add_wrap(" = 0 as number;");
                            } else {
                                result.add_wrap("const ");
                                result.add_fragment(left, value_offset);
                                result.add_wrap(" = __item__;");
                            }
                            result.add_wrap("(");
                            result.add_fragment(right, value_offset + left.len() + " in ".len());
                            result.add_wrap(");");
                        } else {
                            result.add_wrap("(");
                            result.add_fragment(value, value_offset);
                            result.add_wrap(");");
                        }
                    } else if key == "v-slot" || key.starts_with("#") || key.starts_with("v-slot:")
                    {
                        if value.starts_with("{") && value.ends_with("}") {
                            result.add_wrap("{const ");
                        } else {
                            result.add_wrap("{const {");
                        }
                        result.add_fragment(value, value_offset);
                        if value.starts_with("{") && value.ends_with("}") {
                            result.add_wrap(" = {} as Record<string, any>;");
                        } else {
                            result.add_wrap("} = {} as Record<string, any>;");
                        }
                        if close_str == "}" {
                            close_str = "}}";
                        } else {
                            close_str = "}";
                        }
                    } else if key == "slot-scope" {
                        if value.starts_with("{") && value.ends_with("}") {
                            result.add_wrap("{const ");
                        } else {
                            result.add_wrap("{const {");
                        }
                        result.add_fragment(value, value_offset);
                        if value.starts_with("{") && value.ends_with("}") {
                            result.add_wrap(" = {} as Record<string, any>;");
                        } else {
                            result.add_wrap("} = {} as Record<string, any>;");
                        }
                        if close_str == "}" {
                            close_str = "}}";
                        } else {
                            close_str = "}";
                        }
                    } else if (key.starts_with("@") || key.starts_with("v-on:"))
                        && !value.contains("=>")
                    {
                        result.add_wrap("(()=>{");
                        result.add_fragment(value, value_offset);
                        result.add_wrap("});");
                    } else if !skip_util_v_if && !skip_util_v_else_if {
                        result.add_wrap("(");
                        result.add_fragment(value, value_offset);
                        result.add_wrap(");");
                    }
                }
            }
        } else if skip_util_v_if && key == v_if_key {
            skip_util_v_if = false;
        } else if skip_util_v_else_if && key == v_else_if_key {
            skip_util_v_else_if = false;
        }
    }

    let mut start = node.start_tag_end;
    for child in &node.children {
        // 子节点前的文本
        if let Some(start) = start {
            compile_text(start, child.start, source, result);
        }
        compile_node(child, source, result);
        start = Some(child.end);
    }
    // 最后一个子节点后的文本
    if let Some(end) = node.end_tag_start {
        if let Some(start) = start {
            if end > start {
                compile_text(start, end, source, result);
            }
        }
    }

    if close_str.len() > 0 {
        result.add_wrap(close_str);
    }
}

fn compile_text(start: usize, end: usize, source: &str, result: &mut TemplateCompileResult) {
    let text = &source[start..end];
    let mut in_comment = false;
    let mut prev_end = 0;
    for cap in REG_DOUBLE_BRACES.captures_iter(text) {
        let first = cap.get(0).unwrap();
        let prev_text = &text[prev_end..first.start()];
        let mut stream = MultiLineStream::new(prev_text, 0);
        while !stream.eos() {
            if in_comment {
                in_comment = !stream.advance_until_chars("-->");
            } else {
                in_comment = stream.advance_until_chars("<!--");
            }
        }

        prev_end = first.end();
        if !in_comment {
            let m = cap.get(1).unwrap();
            result.add_wrap("(");
            result.add_fragment(m.as_str(), start + m.start());
            result.add_wrap(");");
        }
    }
}

/// 模版编译结果
struct TemplateCompileResult {
    /// 编译输出的 render 方法
    pub render: String,
    /// 编译前后的映射关系
    pub mapping: CompileMapping,
    offset: usize,
}

impl TemplateCompileResult {
    pub fn new() -> Self {
        TemplateCompileResult {
            render: String::new(),
            mapping: vec![],
            offset: 0,
        }
    }

    fn add_wrap(&mut self, target: &str) {
        self.render += &target.replace("\r", " ").replace("\n", " ");
        self.offset += target.len();
    }

    fn add_fragment(&mut self, target: &str, mut original: usize) {
        if target.len() == 0 {
            self.mapping.push((self.offset, original, 0));
            return;
        }
        // 按 $ 分隔，并在随后的步骤中加上 `this.` 前缀
        let mut split = target.split("$");
        // 第一个必定存在，并且不需要加 `this.` 前缀
        let first = split.next().unwrap();
        if first.len() > 0 {
            self.render += &first.replace("\r", " ").replace("\n", " ");
            self.mapping.push((self.offset, original, first.len()));
            self.offset += first.len();
            original += first.len();
        }

        let prefix = "this.";
        let mut is_add = !first.ends_with(".");
        for item in split {
            // 循环中除了 $event 的每项都需要加前缀
            if is_add && !item.starts_with("event") {
                self.add_wrap(prefix);
            }
            self.render += "$";
            self.render += &item.replace("\n", " ");
            self.mapping.push((self.offset, original, item.len() + 1));
            self.offset += item.len() + 1;
            original += item.len() + 1;
            is_add = !item.ends_with(".");
        }
    }
}

/// 映射表，Vec<(character, 原位置, 长度)>
pub type CompileMapping = Vec<(usize, usize, usize)>;

#[cfg(test)]
mod tests {
    use html_languageservice::{parser::html_parse, HTMLDataManager};

    use super::template_compile;

    fn assert_render(template: &str, expected: &str, expected_mapping: &[(usize, usize, usize)]) {
        let html_document =
            html_parse::parse_html_document(template, "html", &HTMLDataManager::default(), true);
        let (render, mapping) = template_compile(&html_document.roots[0], template);
        assert_eq!(render, expected);
        assert_eq!(mapping, expected_mapping.to_vec());
    }

    #[test]
    fn empty_props() {
        assert_render("<template></template>", "", &[]);
        assert_render("<template><div></div></template>", "", &[]);
        assert_render("<template><ProjectHeader /></template>", "", &[]);
    }

    #[test]
    fn with_props() {
        assert_render(
            r#"<template><ProjectHeader title="header" /></template>"#,
            "",
            &[],
        );
        assert_render(
            r#"<template><ProjectHeader :title="title" :job="job" /></template>"#,
            &["(title);", "(job);"].join(""),
            &[(1, 33, 5), (9, 46, 3)],
        );
        assert_render(
            r#"<template><ProjectHeader :title="title" :job="" /></template>"#,
            &["(title);", "();"].join(""),
            &[(1, 33, 5), (9, 46, 0)],
        );
    }

    #[test]
    fn line_breaks() {
        assert_render(
            "<template><ProjectHeader :fields=\"{\n  title: 'text', value: 'v'\n  }\"></ProjectHeader></template>",
            &["({   title: 'text', value: 'v'   });"].join(""),
            &[(1, 34, 33)],
        );
        assert_render(
            "<template><ProjectHeader :fields=\"{\r\n  title: 'text', value: 'v'\r\n  }\"></ProjectHeader></template>",
            &["({    title: 'text', value: 'v'    });"].join(""),
            &[(1, 34, 35)],
        );
    }

    #[test]
    fn directive_if() {
        assert_render(
            r#"<template><ProjectHeader v-if="showHeader" title="header" /><Empty v-else /></template>"#,
            "if(showHeader){}else{}",
            &[(3, 31, 10)],
        );
        assert_render(
            r#"<template><ProjectHeader v-if="showHeader" title="header" /><Empty v-else-if="showEmpty" /></template>"#,
            "if(showHeader){}else if(showEmpty){}",
            &[(3, 31, 10), (24, 78, 9)],
        );
        assert_render(
            r#"<template><ProjectHeader v-if="showHeader" :title="title" /><Empty /></template>"#,
            "if(showHeader){(title);}",
            &[(3, 31, 10), (16, 51, 5)],
        );
        // 位于 v-if 之前的表达式暂时跳过
        assert_render(
            r#"<template><ProjectHeader :title="title" v-if="showHeader" /><Empty /></template>"#,
            "if(showHeader){}",
            &[(3, 46, 10)],
        );
    }

    #[test]
    fn directive_for() {
        assert_render(
            r#"<TabPane :key="item.task.id" v-for="item in tabLists" :closable="true" class="content-tab-pane"></TabPane>"#,
            &[
                "for(const __item__ of tabLists){",
                "(item.task.id);",
                "const item = __item__;",
                "(tabLists);",
                "(true);",
                "}",
            ]
            .join(""),
            &[(33, 15, 12), (53, 36, 4), (70, 44, 8), (81, 65, 4)],
        );
    }

    #[test]
    fn directive_for_with_index() {
        assert_render(
            r#"<div :key="index" v-for="(item, index) in list"></div>"#,
            &[
                "for(const __item__ of list){",
                "(index);",
                "const item = __item__;",
                "const index = 0 as number;",
                "(list);",
                "}",
            ]
            .join(""),
            &[(29, 11, 5), (42, 26, 4), (64, 32, 5), (85, 42, 4)],
        );
    }

    #[test]
    fn single_line_multi_expression() {
        assert_render(
            "<div>{{ one }}{{ two }}</div>",
            &["( one );", "( two );"].join(""),
            &[(1, 7, 5), (9, 16, 5)],
        );
    }

    #[test]
    fn directive_slot_default() {
        assert_render(
            r#"<template v-slot="{ item }"></template>"#,
            "{const { item } = {} as Record<string, any>;}",
            &[(7, 18, 8)],
        );
    }

    #[test]
    fn directive_slot_name() {
        assert_render(
            r#"<template v-slot:name="{ item }"></template>"#,
            "{const { item } = {} as Record<string, any>;}",
            &[(7, 23, 8)],
        );
        assert_render(
            r#"<template #name="{ item }"></template>"#,
            "{const { item } = {} as Record<string, any>;}",
            &[(7, 17, 8)],
        );
        assert_render(
            r#"<template #name="value, record"></template>"#,
            "{const {value, record} = {} as Record<string, any>;}",
            &[(8, 17, 13)],
        );
    }

    #[test]
    fn directive_slot_scope() {
        assert_render(
            r#"<template slot-scope="record"></template>"#,
            "{const {record} = {} as Record<string, any>;}",
            &[(8, 22, 6)],
        );
        assert_render(
            r#"<template slot-scope="{ prop }"></template>"#,
            "{const { prop } = {} as Record<string, any>;}",
            &[(7, 22, 8)],
        );
        assert_render(
            r#"<template v-if="show" slot-scope="record"></template>"#,
            "if(show){{const {record} = {} as Record<string, any>;}}",
            &[(3, 16, 4), (17, 34, 6)],
        );
    }

    #[test]
    fn event() {
        assert_render(
            r#"<div @click="onClick"></div>"#,
            "(()=>{onClick});",
            &[(6, 13, 7)],
        );
        assert_render(
            r#"<div @click="onClick()"></div>"#,
            "(()=>{onClick()});",
            &[(6, 13, 9)],
        );
        assert_render(
            r#"<div @click="e => onClick(e)"></div>"#,
            "(e => onClick(e));",
            &[(1, 13, 15)],
        );
        assert_render(
            r#"<div @click="value = 'xxx'"></div>"#,
            "(()=>{value = 'xxx'});",
            &[(6, 13, 13)],
        );
    }

    #[test]
    fn symbol() {
        assert_render(
            r#"<div :title="$route.query.title"></div>"#,
            "(this.$route.query.title);",
            &[(6, 13, 18)],
        );
        assert_render(
            r#"<div :title="$parent.$parent.title"></div>"#,
            "(this.$parent.$parent.title);",
            &[(6, 13, 8), (14, 21, 13)],
        );
        assert_render(
            r#"<div @click="onClick($event)"></div>"#,
            "(()=>{onClick($event)});",
            &[(6, 13, 8), (14, 21, 7)],
        );
    }

    #[test]
    fn comment() {
        assert_render(
            "<div>{{ one }}<!-- {{ two }} --></div>",
            &["( one );"].join(""),
            &[(1, 7, 5)],
        );
        assert_render(
            "<div>{{ one }}<!-- {{ two }} -->{{ three }}</div>",
            &["( one );", "( three );"].join(""),
            &[(1, 7, 5), (9, 34, 7)],
        );
        assert_render(
            "<div>{{ one }}<!-- xxxxxxxxx -->{{ three }}</div>",
            &["( one );", "( three );"].join(""),
            &[(1, 7, 5), (9, 34, 7)],
        );
    }
}
