use swc_ecma_ast::{ClassDecl, Decl, ExportDecl};

use super::is_specified_decorator;

pub fn get_ident_from_export_decl(export_decl: &ExportDecl) -> String {
    match &export_decl.decl {
        swc_ecma_ast::Decl::Class(class_decl) => class_decl.ident.to_string(),
        swc_ecma_ast::Decl::Fn(fn_decl) => fn_decl.ident.to_string(),
        swc_ecma_ast::Decl::Var(_) => String::new(),
        swc_ecma_ast::Decl::Using(_) => String::new(),
        swc_ecma_ast::Decl::TsInterface(ts_interface_decl) => ts_interface_decl.id.to_string(),
        swc_ecma_ast::Decl::TsTypeAlias(ts_type_alias_decl) => ts_type_alias_decl.id.to_string(),
        swc_ecma_ast::Decl::TsEnum(ts_enum_decl) => ts_enum_decl.id.to_string(),
        swc_ecma_ast::Decl::TsModule(_) => String::new(),
    }
}

/// 获取导出的 class 组件
pub fn get_export_class_component_from_export_decl(export_decl: &ExportDecl) -> Option<&ClassDecl> {
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
