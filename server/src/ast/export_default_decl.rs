use swc_ecma_ast::{ClassExpr, DefaultDecl, ExportDefaultDecl};

use crate::ast::is_specified_decorator;

/// 获取导出的 class 组件
pub fn get_export_class_component_from_export_default_decl(
    export_decl: &ExportDefaultDecl,
) -> Option<&ClassExpr> {
    if let DefaultDecl::Class(class) = &export_decl.decl {
        if class
            .class
            .decorators
            .iter()
            .find(|d| is_specified_decorator(d, "Component"))
            .is_some()
        {
            return Some(class);
        }
    }
    None
}
