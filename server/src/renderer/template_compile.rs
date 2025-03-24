use html_languageservice::parser::html_document::Node;

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
                    if let Some(caps) = REG_V_FOR_WITH_INDEX.captures(left) {
                        let item = caps.get(1).unwrap().as_str();
                        let index = caps.get(2).unwrap().as_str();
                        result.add_wrap(&format!("let {index} = 0;"));
                        result.add_wrap(&format!("for(const {item} of {right})"));
                        result.add_wrap("{");
                        close_str = "index+=1;}"
                    } else {
                        result.add_wrap(&format!("for(const {left} of {right})"));
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
                    if key == v_if_key {
                        skip_util_v_if = false;
                    } else if key == v_else_if_key {
                        skip_util_v_else_if = false;
                    }
                    if key == v_for_key {
                        if let Some((left, right)) = value.split_once(" in ") {
                            if let Some(caps) = REG_V_FOR_WITH_INDEX.captures(left) {
                                let item = caps.get(1).unwrap();
                                let index = caps.get(2).unwrap();
                                result.add_wrap("(");
                                result.add_fragment(item.as_str(), value_offset + item.start());
                                result.add_wrap(");");
                                result.add_wrap("(");
                                result.add_fragment(index.as_str(), value_offset + index.start());
                                result.add_wrap(");");
                            } else {
                                result.add_wrap("(");
                                result.add_fragment(left, value_offset);
                                result.add_wrap(");");
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
                        result.add_wrap("{const ");
                        result.add_fragment(value, value_offset);
                        result.add_wrap(" = {} as Record<string, any>;");
                        close_str = "}";
                    } else if key == "slot-scope" {
                        result.add_wrap("{const {");
                        result.add_fragment(value, value_offset);
                        result.add_wrap("} = {} as Record<string, any>;");
                        close_str = "}";
                    } else if !skip_util_v_if && !skip_util_v_else_if {
                        result.add_wrap("(");
                        result.add_fragment(value, value_offset);
                        result.add_wrap(");");
                    }
                }
            }
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
    for cap in REG_DOUBLE_BRACES.captures_iter(text) {
        let m = cap.get(1).unwrap();
        result.add_wrap("(");
        result.add_fragment(m.as_str(), start + m.start());
        result.add_wrap(");");
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
        self.render += &target.replace("\n", " ");
        self.offset += target.len();
    }

    fn add_fragment(&mut self, target: &str, mut original: usize) {
        // 按 $ 分隔，并在随后的步骤中加上 `this.` 前缀
        let mut split = target.split("$");
        // 第一个必定存在，并且不需要加 `this.` 前缀
        let first = split.next().unwrap();
        self.render += &first.replace("\n", " ");
        self.mapping.push((self.offset, original, first.len()));
        self.offset += first.len();
        original += first.len();

        let prefix = "this.";
        for item in split {
            // 循环中除了 $event 的每项都需要加前缀
            if !item.starts_with("event") {
                self.add_wrap(prefix);
            }
            self.render += "$";
            self.render += &item.replace("\n", " ");
            self.mapping.push((self.offset, original, item.len() + 1));
            self.offset += item.len() + 1;
            original += item.len() + 1;
        }
    }
}

/// 映射表，Vec<(character, 原位置, 长度)>
pub type CompileMapping = Vec<(usize, usize, usize)>;

#[cfg(test)]
mod tests {
    use html_languageservice::HTMLDataManager;

    use super::template_compile;

    fn assert_render(template: &str, expected: &str, expected_mapping: &[(usize, usize, usize)]) {
        let html_document = html_languageservice::parse_html_document(
            template,
            "html",
            &HTMLDataManager::default(),
        );
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
                "for(const item of tabLists){",
                "(item.task.id);",
                "(item);",
                "(tabLists);",
                "(true);",
                "}",
            ]
            .join(""),
            &[(29, 15, 12), (44, 36, 4), (51, 44, 8), (62, 65, 4)],
        );
    }

    #[test]
    fn directive_for_with_index() {
        assert_render(
            r#"<div :key="index" v-for="(item, index) in list"></div>"#,
            &[
                "let index = 0;",
                "for(const item of list){",
                "(index);",
                "(item);",
                "(index);",
                "(list);",
                "index+=1;",
                "}",
            ]
            .join(""),
            &[(39, 11, 5), (47, 26, 4), (54, 32, 5), (62, 42, 4)],
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
    }

    #[test]
    fn directive_slot_scope() {
        assert_render(
            r#"<template slot-scope="record"></template>"#,
            "{const {record} = {} as Record<string, any>;}",
            &[(8, 22, 6)],
        );
    }
}
