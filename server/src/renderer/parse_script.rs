use swc_common::{source_map::SmallPos, Span};
use swc_ecma_ast::Module;

use crate::ast;

/// 解析脚本，输出 props, render_insert_offset, extends_component, registers
pub fn parse_script(source: &str, start_pos: usize, end_pos: usize) -> Option<ParseScriptResult> {
    let (module, _) = ast::parse_source(source, start_pos, end_pos);
    if let Ok(module) = &module {
        parse_module(module)
    } else {
        None
    }
}

pub fn parse_module(module: &Module) -> Option<ParseScriptResult> {
    let mut extends_component = None;
    if let Some(class) = ast::get_default_class_expr_from_module(module) {
        let mut props = vec![];
        for member in class
            .class
            .body
            .iter()
            .filter(|v| ast::filter_all_prop_method(v))
            .collect::<Vec<_>>()
        {
            props.push(ast::get_class_member_name(member));
        }
        let extends_ident = ast::get_extends_component(class);
        if let Some(extends_ident) = extends_ident {
            if let Some((orig_name, path)) = ast::get_import_from_module(module, &extends_ident) {
                if !orig_name.as_ref().is_some_and(|v| v == "Vue") {
                    extends_component = Some(ExtendsComponent {
                        export_name: orig_name,
                        path,
                    });
                }
            }
        }
        let render_insert_offset = class.class.span.hi.to_usize() - 1;
        let mut registers = vec![];
        let registered_components = ast::get_registered_components(module, class).unwrap_or(vec![]);
        for (name, export, prop, path) in registered_components {
            registers.push(RegisterComponent {
                name,
                export,
                prop,
                path,
            });
        }
        Some(ParseScriptResult {
            name_span: class.class.span,
            props,
            render_insert_offset,
            extends_component,
            registers,
        })
    } else {
        None
    }
}

/// 继承的组件
#[derive(Debug, PartialEq)]
pub struct ExtendsComponent {
    /// 导出的组件名，如果是默认导出，则为 None，如果被重命名，那么则为重命名前的名称
    pub export_name: Option<String>,
    /// 导入路径
    pub path: String,
}

/// 注册的组件
#[derive(Debug, PartialEq, Clone)]
pub struct RegisterComponent {
    /// 注册的名称
    pub name: String,
    /// 导出的名称
    pub export: Option<String>,
    /// `导出的名称的属性`
    /// 如果是使用类似 Select.Option 注册的，
    /// 那么 prop 是 Some("Option"), export_name 是 Some("Select")，
    pub prop: Option<String>,
    /// 导入路径
    pub path: String,
}

pub struct ParseScriptResult {
    pub name_span: Span,
    pub props: Vec<String>,
    pub render_insert_offset: usize,
    pub extends_component: Option<ExtendsComponent>,
    pub registers: Vec<RegisterComponent>,
}

#[cfg(test)]
mod tests {
    use super::{ExtendsComponent, RegisterComponent};

    fn assert_props(source: &str, expected: &[&str]) {
        let props = super::parse_script(source, 0, source.len()).unwrap().props;
        assert_eq!(
            props,
            expected.iter().map(|v| v.to_string()).collect::<Vec<_>>()
        );
    }

    fn assert_render_insert_offset(source: &str, expected: usize) {
        let render_insert_offset = super::parse_script(source, 0, source.len())
            .unwrap()
            .render_insert_offset;
        assert_eq!(render_insert_offset, expected);
    }

    fn assert_extends_component(source: &str, expected: Option<(Option<&str>, &str)>) {
        let extends_component = super::parse_script(source, 0, source.len())
            .unwrap()
            .extends_component;
        assert_eq!(
            extends_component,
            expected.map(|v| ExtendsComponent {
                export_name: v.0.map(|s| s.to_string()),
                path: v.1.to_string(),
            })
        )
    }

    fn assert_registers(source: &str, expected: &[RegisterComponent]) {
        let registers = super::parse_script(source, 0, source.len())
            .unwrap()
            .registers;
        assert_eq!(registers, expected.to_vec())
    }

    #[test]
    fn normal() {
        let source = &[
            "import MyComponent1 from './components/MyComponent1.vue'",
            "import MyComponent2 from './components/MyComponent2.vue'",
            "@Component({",
            "    components: {",
            "        MyComponent1,",
            "        MyComponent2,",
            "    },",
            "})",
            "export default class Test extends Vue {",
            "   private prop1 = ''",
            "   public prop2 = 1",
            "   protected get prop3() {",
            "       return true",
            "   }",
            "   private method1() {}",
            "   private method2() {",
            "       console.log('method2')",
            "   }",
            "}",
        ]
        .join("\n");
        assert_props(source, &["prop1", "prop2", "prop3", "method1", "method2"]);
        assert_render_insert_offset(source, 414);
        assert_extends_component(source, None);
        assert_registers(
            source,
            &[
                RegisterComponent {
                    name: "MyComponent1".to_string(),
                    export: None,
                    prop: None,
                    path: "./components/MyComponent1.vue".to_string(),
                },
                RegisterComponent {
                    name: "MyComponent2".to_string(),
                    export: None,
                    prop: None,
                    path: "./components/MyComponent2.vue".to_string(),
                },
            ],
        );
    }

    #[test]
    fn extends_component() {
        let source = &[
            "import MyComponent1 from './components/MyComponent1.vue'",
            "import MyComponent2 from './components/MyComponent2.vue'",
            "@Component({",
            "    components: {",
            "        MyComponent2,",
            "    },",
            "})",
            "export default class Test extends MyComponent1 {",
            "   private prop1 = ''",
            "   public prop2 = 1",
            "   protected get prop3() {",
            "       return true",
            "   }",
            "   private method1() {}",
            "   private method2() {",
            "       console.log('method2')",
            "   }",
            "}",
        ]
        .join("\n");
        assert_extends_component(source, Some((None, "./components/MyComponent1.vue")));
    }

    #[test]
    fn with_lib_component() {
        let source = &[
            "import { Button, Select } from 'component-library'",
            "import MyComponent1 from './components/MyComponent1.vue'",
            "import MyComponent2 from './components/MyComponent2.vue'",
            "@Component({",
            "    components: {",
            "        Button,",
            "        Select,",
            "        MyComponent1,",
            "        MyComponent2,",
            "    },",
            "})",
            "export default class Test extends Vue {",
            "   private prop1 = ''",
            "   public prop2 = 1",
            "   protected get prop3() {",
            "       return true",
            "   }",
            "   private method1() {}",
            "   private method2() {",
            "       console.log('method2')",
            "   }",
            "}",
        ]
        .join("\n");
        assert_registers(
            source,
            &[
                RegisterComponent {
                    name: "Button".to_string(),
                    export: Some("Button".to_string()),
                    prop: None,
                    path: "component-library".to_string(),
                },
                RegisterComponent {
                    name: "Select".to_string(),
                    export: Some("Select".to_string()),
                    prop: None,
                    path: "component-library".to_string(),
                },
                RegisterComponent {
                    name: "MyComponent1".to_string(),
                    export: None,
                    prop: None,
                    path: "./components/MyComponent1.vue".to_string(),
                },
                RegisterComponent {
                    name: "MyComponent2".to_string(),
                    export: None,
                    prop: None,
                    path: "./components/MyComponent2.vue".to_string(),
                },
            ],
        );
    }

    #[test]
    fn with_mixins() {
        let source = &[
            "import MyComponent1 from './components/MyComponent1.vue'",
            "import MyComponent2 from '@components/MyComponent2.vue'",
            "@Component({",
            "    components: {",
            "        MyComponent1,",
            "    },",
            "    mixins: [MyComponent2],",
            "})",
            "export default class Test extends Vue {",
            "   private prop1 = ''",
            "   public prop2 = 1",
            "   protected get prop3() {",
            "       return true",
            "   }",
            "   private method1() {}",
            "   private method2() {",
            "       console.log('method2')",
            "   }",
            "}",
        ]
        .join("\n");
        assert_registers(
            source,
            &[RegisterComponent {
                name: "MyComponent1".to_string(),
                export: None,
                prop: None,
                path: "./components/MyComponent1.vue".to_string(),
            }],
        );
    }
}
