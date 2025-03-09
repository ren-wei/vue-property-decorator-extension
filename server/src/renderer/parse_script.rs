use swc_common::source_map::SmallPos;
use swc_ecma_ast::Module;

use crate::ast;

/// 解析脚本，输出 props, render_insert_offset, extends_component, registers
pub fn parse_script(
    source: &str,
    start_pos: usize,
    end_pos: usize,
) -> Option<(
    Vec<String>,
    usize,
    Option<ExtendsComponent>,
    Vec<RegisterComponent>,
)> {
    let (module, _) = ast::parse_source(source, start_pos, end_pos);
    if let Ok(module) = &module {
        parse_module(module)
    } else {
        None
    }
}

pub fn parse_module(
    module: &Module,
) -> Option<(
    Vec<String>,
    usize,
    Option<ExtendsComponent>,
    Vec<RegisterComponent>,
)> {
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
        // TODO: 解析 registers
        Some((props, render_insert_offset, extends_component, registers))
    } else {
        None
    }
}

/// 继承的组件
#[derive(Debug)]
pub struct ExtendsComponent {
    /// 导出的组件名，如果是默认导出，则为 None，如果被重命名，那么则为重命名前的名称
    pub export_name: Option<String>,
    /// 导入路径
    pub path: String,
}

/// 注册的组件
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
