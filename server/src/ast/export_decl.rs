use swc_ecma_ast::{ClassDecl, Decl, ExportDecl};

use super::{get_ident_from_decl, is_specified_decorator};

pub fn get_ident_from_export_decl(export_decl: &ExportDecl) -> String {
    get_ident_from_decl(&export_decl.decl)
}

/// 获取导出的 class 组件
pub fn _get_export_class_component_from_export_decl(
    export_decl: &ExportDecl,
) -> Option<&ClassDecl> {
    if let Decl::Class(class) = &export_decl.decl {
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
