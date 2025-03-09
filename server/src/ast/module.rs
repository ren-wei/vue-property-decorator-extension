use swc_ecma_ast::{
    ClassDecl, ClassExpr, Decl, DefaultDecl, ExportAll, ExportSpecifier, Expr, ImportDecl,
    ImportSpecifier, Module, ModuleDecl, ModuleExportName, ModuleItem, Prop, PropOrSpread,
};

use super::{
    decorator::{get_decorator_args, is_specified_decorator},
    expr::get_object_props,
    get_export_class_component_from_export_decl,
    get_export_class_component_from_export_default_decl, get_export_from_export_specifier,
    get_ident_from_export_decl, get_local_from_import_specifier,
    get_orig_name_from_export_specifier, get_orig_name_from_import_specifier,
    import::get_specified_import,
    prop_name::get_name_form_prop_name,
    prop_or_spread::get_value_of_specified_prop,
};

/// 获取注册的组件映射关系
pub fn get_registered_components(
    module: &Module,
    class: &ClassExpr,
) -> Option<Vec<ModuleReference>> {
    // import
    let imports = get_import_expr(&module);

    let component_decorator = class
        .class
        .decorators
        .iter()
        .find(|decorator| is_specified_decorator(decorator, "Component"));
    if component_decorator.is_none() {
        return None;
    }
    let component_decorator = component_decorator.unwrap();
    // collect module_references
    let mut module_references = vec![];
    let args = get_decorator_args(&component_decorator)?;
    let arg = &args[0];
    let props = get_object_props(arg.expr.as_ref())?;
    for prop in props {
        let value = get_value_of_specified_prop(prop, "components")?;
        let props = get_object_props(value)?;
        for prop in props {
            if let PropOrSpread::Prop(prop) = prop {
                let name;
                match prop.as_ref() {
                    Prop::Shorthand(prop) => {
                        name = prop.sym.to_string();
                    }
                    Prop::KeyValue(prop) => {
                        name = get_name_form_prop_name(&prop.key);
                    }
                    _ => name = "unknown".to_string(),
                }
                if let Some((import, raw_path)) = get_specified_import(&imports, &name) {
                    let export;
                    match import {
                        ImportSpecifier::Default(_) => {
                            export = None;
                        }
                        ImportSpecifier::Named(import) => {
                            if let Some(imported) = &import.imported {
                                export = Some(match imported {
                                    ModuleExportName::Ident(ident) => ident.sym.to_string(),
                                    ModuleExportName::Str(s) => s.value.to_string(),
                                });
                            } else {
                                export = Some(import.local.sym.to_string());
                            }
                        }
                        ImportSpecifier::Namespace(_) => {
                            continue;
                        }
                    }
                    module_references.push(ModuleReference {
                        name,
                        export,
                        raw_path: raw_path.to_string(),
                    });
                }
            }
        }
    }
    Some(module_references)
}

/// 如果导出存在，那么返回 OK；
/// 如果导出的表达式来自当前模块，那么返回 None；否则，是来自引用，返回引用的导出和真实路径
pub fn get_registered_component(
    name: &str,
    module: &Module,
    export: Option<String>,
) -> Result<Option<ModuleReference>, String> {
    for body in &module.body {
        // 递归导出
        if let ModuleItem::ModuleDecl(module_decl) = body {
            if let ModuleDecl::ExportNamed(export_named) = module_decl {
                if let Some(src) = &export_named.src {
                    for specifier in &export_named.specifiers {
                        match specifier {
                            ExportSpecifier::Default(_) => {
                                if export.is_none() {
                                    return Ok(Some(ModuleReference {
                                        name: name.to_string(),
                                        export: None,
                                        raw_path: src.value.to_string(),
                                    }));
                                }
                            }
                            ExportSpecifier::Named(specifier) => {
                                if let Some(export) = export.as_deref() {
                                    let export_name = if let Some(exported) = &specifier.exported {
                                        get_module_export_name(exported)
                                    } else {
                                        get_module_export_name(&specifier.orig)
                                    };
                                    if export_name == export {
                                        return Ok(Some(ModuleReference {
                                            name: name.to_string(),
                                            export: Some(get_module_export_name(&specifier.orig)),
                                            raw_path: src.value.to_string(),
                                        }));
                                    }
                                }
                            }
                            ExportSpecifier::Namespace(specifier) => {
                                if let Some(export) = export.as_deref() {
                                    let specifier_name = get_module_export_name(&specifier.name);
                                    if specifier_name == export {
                                        return Ok(Some(ModuleReference {
                                            name: name.to_string(),
                                            export: Some(specifier_name),
                                            raw_path: src.value.to_string(),
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // 语句
    }
    Err("(get_registered_component) module not find".to_string())
}

pub fn get_import_expr(module: &Module) -> Vec<&ImportDecl> {
    let mut imports = vec![];
    for item in &module.body {
        if let ModuleItem::ModuleDecl(decl) = item {
            if let ModuleDecl::Import(import) = decl {
                imports.push(import);
            }
        }
    }

    imports
}

pub fn get_export_all(module: &Module) -> Option<&ExportAll> {
    for item in &module.body {
        if let ModuleItem::ModuleDecl(decl) = item {
            if let ModuleDecl::ExportAll(export_all) = decl {
                return Some(export_all);
            }
        }
    }
    None
}

/// 获取所有存在导出的导入项，作为模块引用
pub fn get_export_module_reference(module: &Module) -> Vec<ModuleReference> {
    // 1. 先获取所有导入导出项
    // 2. 获取存在导出项的导入项并转换为模块引用
    let module_decls = module
        .body
        .iter()
        .filter(|item| item.is_module_decl())
        .map(|item| item.as_module_decl().unwrap());
    let mut export_list = vec![];
    let mut import_list = vec![];
    module_decls.for_each(|item| {
        if let ModuleDecl::Import(import) = item {
            for specifier in &import.specifiers {
                if let ImportSpecifier::Named(named) = specifier {
                    import_list.push(ModuleReference {
                        name: named.local.sym.to_string(),
                        export: Some(named.local.sym.to_string()),
                        raw_path: import.src.value.to_string(),
                    });
                }
            }
        } else if let ModuleDecl::ExportNamed(export) = item {
            for specifier in &export.specifiers {
                if let ExportSpecifier::Named(named) = specifier {
                    export_list.push(get_module_export_name(&named.orig));
                }
            }
        }
    });
    let mut list = vec![];
    for item in import_list {
        if let Some(export) = &item.export {
            if export_list.contains(&export) {
                list.push(item);
            }
        }
    }
    list
}

pub fn get_default_class_expr_from_module(module: &Module) -> Option<&ClassExpr> {
    for item in module.body.iter() {
        if let ModuleItem::ModuleDecl(item) = item {
            if let ModuleDecl::ExportDefaultDecl(item) = item {
                if let DefaultDecl::Class(item) = &item.decl {
                    return Some(item);
                }
            }
        }
    }
    None
}

pub fn get_class_decl_from_module(module: Module, export: &Option<String>) -> Option<ClassDecl> {
    for item in module.body {
        if let ModuleItem::ModuleDecl(item) = item {
            if let ModuleDecl::ExportDecl(item) = item {
                if let Decl::Class(item) = item.decl {
                    if &Some(item.ident.sym.to_string()) == export {
                        return Some(item);
                    }
                }
            }
        }
    }
    None
}

pub fn get_module_export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(name) => name.sym.to_string(),
        ModuleExportName::Str(name) => name.value.to_string(),
    }
}

/// 模块引用
#[derive(Debug, Clone)]
pub struct ModuleReference {
    /// 实际使用的名称
    pub name: String,
    /// 导出的名
    pub export: Option<String>,
    /// 真实路径
    pub raw_path: String,
}

// ---- new ----

/// 从 module 获取导入的原始名称和路径
pub fn get_import_from_module(module: &Module, name: &String) -> Option<(Option<String>, String)> {
    for body in &module.body {
        if let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = body {
            for specifier in &import_decl.specifiers {
                if let Some(orig_name) = get_orig_name_from_import_specifier(specifier) {
                    if name == &get_local_from_import_specifier(specifier) {
                        return Some((orig_name, import_decl.src.value.to_string()));
                    }
                }
            }
        }
    }
    None
}

/// 从 module 获取导出
pub fn get_export_from_module(module: &Module, export_name: &Option<String>) -> TsFileExportResult {
    // (export_name, orig_name, path)
    let mut imports: Vec<(String, Option<String>, String)> = vec![];
    let mut export_all_list = vec![];
    for body in &module.body {
        if let ModuleItem::ModuleDecl(module_decl) = body {
            match module_decl {
                ModuleDecl::Import(import_decl) => {
                    for specifier in &import_decl.specifiers {
                        if let Some(orig_name) = get_orig_name_from_import_specifier(specifier) {
                            imports.push((
                                get_local_from_import_specifier(specifier),
                                orig_name,
                                import_decl.src.value.to_string(),
                            ));
                        }
                    }
                }
                ModuleDecl::ExportDecl(export_decl) => {
                    if let Some(export_name) = export_name {
                        if export_name == &get_ident_from_export_decl(export_decl) {
                            let class_decl =
                                get_export_class_component_from_export_decl(export_decl);
                            if class_decl.is_some() {
                                return TsFileExportResult::Current;
                            } else {
                                return TsFileExportResult::None;
                            }
                        }
                    }
                }
                ModuleDecl::ExportNamed(named_export) => {
                    for specifier in &named_export.specifiers {
                        let export = get_export_from_export_specifier(specifier);
                        if let Some(export) = export {
                            if &export == export_name {
                                if let Some(orig_name) =
                                    get_orig_name_from_export_specifier(specifier)
                                {
                                    if let Some(src) = &named_export.src {
                                        return TsFileExportResult::Other(
                                            src.value.to_string(),
                                            orig_name,
                                        );
                                    } else {
                                        // 从 imports 取值
                                        let target = imports.iter().find(|v| {
                                            if let Some(export_name) = export_name {
                                                &v.0 == export_name
                                            } else {
                                                v.0 == "default"
                                            }
                                        });
                                        if let Some((_, orig_name, path)) = target {
                                            return TsFileExportResult::Other(
                                                path.clone(),
                                                orig_name.clone(),
                                            );
                                        }
                                    }
                                }
                                return TsFileExportResult::None;
                            }
                        }
                    }
                }
                ModuleDecl::ExportDefaultDecl(export_default_decl) => {
                    if export_name == &None {
                        if get_export_class_component_from_export_default_decl(export_default_decl)
                            .is_some()
                        {
                            return TsFileExportResult::Current;
                        } else {
                            return TsFileExportResult::None;
                        }
                    }
                }
                ModuleDecl::ExportDefaultExpr(expr) => {
                    if export_name == &None {
                        if let Expr::Ident(indent) = expr.expr.as_ref() {
                            let ident = indent.sym.to_string();
                            let target = imports.iter().find(|v| v.0 == ident);
                            if let Some((_, orig_name, path)) = target {
                                return TsFileExportResult::Other(
                                    path.to_string(),
                                    orig_name.clone(),
                                );
                            }
                        }
                        return TsFileExportResult::None;
                    }
                }
                ModuleDecl::ExportAll(export_all) => {
                    export_all_list.push(export_all.src.value.to_string());
                }
                ModuleDecl::TsImportEquals(_) => {}
                ModuleDecl::TsExportAssignment(_) => {}
                ModuleDecl::TsNamespaceExport(_) => {}
            }
        }
    }
    if export_all_list.len() > 0 {
        TsFileExportResult::Possible(export_all_list)
    } else {
        TsFileExportResult::None
    }
}

pub fn get_local_exports_and_transfers(
    module: &Module,
) -> (
    Vec<Option<String>>,
    Vec<(Option<String>, Option<String>, String, bool)>,
) {
    let mut local_exports = vec![];
    // Vec<(local, export_name, path, is_star_export)>
    let mut transfers = vec![];
    // (export_name, orig_name, path)
    let mut imports: Vec<(String, Option<String>, String)> = vec![];
    for body in &module.body {
        if let ModuleItem::ModuleDecl(module_decl) = body {
            match module_decl {
                ModuleDecl::Import(import_decl) => {
                    for specifier in &import_decl.specifiers {
                        if let Some(orig_name) = get_orig_name_from_import_specifier(specifier) {
                            imports.push((
                                get_local_from_import_specifier(specifier),
                                orig_name,
                                import_decl.src.value.to_string(),
                            ));
                        }
                    }
                }
                ModuleDecl::ExportDecl(export_decl) => {
                    let export_ident = get_ident_from_export_decl(export_decl);
                    if let Some(import) = imports.iter().find(|v| v.0 == export_ident) {
                        transfers.push((
                            Some(export_ident),
                            import.1.clone(),
                            import.2.clone(),
                            false,
                        ));
                    } else {
                        local_exports.push(Some(export_ident));
                    }
                }
                ModuleDecl::ExportNamed(named_export) => {
                    for specifier in &named_export.specifiers {
                        if let Some(export) = get_export_from_export_specifier(specifier) {
                            if let Some(orig_name) = get_orig_name_from_export_specifier(specifier)
                            {
                                if let Some(src) = &named_export.src {
                                    transfers.push((
                                        export,
                                        orig_name,
                                        src.value.to_string(),
                                        false,
                                    ));
                                } else {
                                    // 没有 src 则是从 imports 导出或者本地定义
                                    if let Some(import) =
                                        imports.iter().find(|v| Some(v.0.clone()) == orig_name)
                                    {
                                        transfers.push((
                                            export,
                                            import.1.clone(),
                                            import.2.clone(),
                                            false,
                                        ));
                                    } else {
                                        local_exports.push(export);
                                    }
                                }
                            }
                        }
                    }
                }
                ModuleDecl::ExportDefaultDecl(_) => {
                    local_exports.push(None);
                }
                ModuleDecl::ExportDefaultExpr(expr) => {
                    if let Expr::Ident(indent) = expr.expr.as_ref() {
                        let ident = indent.sym.to_string();
                        let import = imports.iter().find(|v| v.0 == ident);
                        if let Some((_, orig_name, path)) = import {
                            transfers.push((None, orig_name.clone(), path.clone(), false));
                        }
                    }
                }
                ModuleDecl::ExportAll(export_all) => {
                    if let Some(with) = &export_all.with {
                        // TODO: 其他类型的星导出
                    } else {
                        transfers.push((None, None, export_all.src.value.to_string(), true));
                    }
                }
                ModuleDecl::TsImportEquals(_) => {}
                ModuleDecl::TsExportAssignment(_) => {}
                ModuleDecl::TsNamespaceExport(_) => {}
            }
        }
    }

    (local_exports, transfers)
}

/// ts 文件导出解析结果
#[derive(PartialEq, Debug)]
pub enum TsFileExportResult {
    /// 定义自当前文件
    Current,
    /// 明确从其他文件导入
    Other(String, Option<String>),
    /// 未找到指定的导出
    None,
    /// 使用了 `export * from "xxx"` 时可能是来自其他文件的导出
    Possible(Vec<String>),
}

#[cfg(test)]
mod tests {
    use crate::ast;

    use super::{get_export_from_module, TsFileExportResult};

    fn assert_export_result(
        source: &str,
        export_name: &Option<String>,
        expected: TsFileExportResult,
    ) {
        let (module, _) = ast::parse_source(source, 0, source.len());
        let module = module.unwrap();
        let result = get_export_from_module(&module, export_name);
        assert_eq!(result, expected);
    }

    #[test]
    fn current_export_default() {
        assert_export_result(
            &[
                "import { Component } from 'vue-property-decorator';",
                "@Component",
                "export default class MyComponent {}",
            ]
            .join("\n"),
            &None,
            TsFileExportResult::Current,
        );
    }

    #[test]
    fn other_export_default() {
        assert_export_result(
            &["export { MyComponent as default } from 'xxx';"].join("\n"),
            &None,
            TsFileExportResult::Other("xxx".to_string(), Some("MyComponent".to_string())),
        );
        assert_export_result(
            &[
                "import { MyComponent } from 'xxx';",
                "export default MyComponent;",
            ]
            .join("\n"),
            &None,
            TsFileExportResult::Other("xxx".to_string(), Some("MyComponent".to_string())),
        );
        assert_export_result(
            &[
                "import MyComponent from 'xxx';",
                "export default MyComponent;",
            ]
            .join("\n"),
            &None,
            TsFileExportResult::Other("xxx".to_string(), None),
        );
        assert_export_result(
            &[
                "import { OtherComponent as MyComponent } from 'xxx';",
                "export default MyComponent;",
            ]
            .join("\n"),
            &None,
            TsFileExportResult::Other("xxx".to_string(), Some("OtherComponent".to_string())),
        );
    }

    #[test]
    fn other_export_some() {
        assert_export_result(
            &["export { MyComponent } from 'xxx';"].join("\n"),
            &Some("MyComponent".to_string()),
            TsFileExportResult::Other("xxx".to_string(), Some("MyComponent".to_string())),
        );
        assert_export_result(
            &[
                "import { MyComponent } from 'xxx';",
                "export { MyComponent };",
            ]
            .join("\n"),
            &Some("MyComponent".to_string()),
            TsFileExportResult::Other("xxx".to_string(), Some("MyComponent".to_string())),
        );
        assert_export_result(
            &[
                "export * from 'xxx';",
                "import { OtherComponent as MyComponent } from 'xxx';",
                "export { MyComponent };",
            ]
            .join("\n"),
            &Some("MyComponent".to_string()),
            TsFileExportResult::Other("xxx".to_string(), Some("OtherComponent".to_string())),
        );
        assert_export_result(
            &[
                "export * from 'xxx';",
                "import MyComponent from 'xxx';",
                "export { MyComponent };",
            ]
            .join("\n"),
            &Some("MyComponent".to_string()),
            TsFileExportResult::Other("xxx".to_string(), None),
        );
    }

    #[test]
    fn export_none() {
        assert_export_result(
            &["export { MyComponent } from 'xxx';"].join("\n"),
            &None,
            TsFileExportResult::None,
        );
        assert_export_result(
            &[
                "import { MyComponent } from 'xxx';",
                "export { MyComponent };",
            ]
            .join("\n"),
            &None,
            TsFileExportResult::None,
        );
        assert_export_result(
            &[
                "import { OtherComponent as MyComponent } from 'xxx';",
                "export { MyComponent };",
            ]
            .join("\n"),
            &None,
            TsFileExportResult::None,
        );
        assert_export_result(
            &["import MyComponent from 'xxx';", "export { MyComponent };"].join("\n"),
            &None,
            TsFileExportResult::None,
        );
        assert_export_result(
            &[
                "import { OtherComponent as MyComponent } from 'xxx';",
                "export { MyComponent };",
            ]
            .join("\n"),
            &Some("OtherComponent".to_string()),
            TsFileExportResult::None,
        );
        assert_export_result(
            &[
                "import { Component } from 'vue-property-decorator';",
                "export default {};",
            ]
            .join("\n"),
            &None,
            TsFileExportResult::None,
        );
        assert_export_result(
            &[
                "import { Component } from 'vue-property-decorator';",
                "export const MyComponent = {};",
            ]
            .join("\n"),
            &Some("MyComponent".to_string()),
            TsFileExportResult::None,
        );
    }

    #[test]
    fn export_possible() {
        assert_export_result(
            &["export * from 'xxx';"].join("\n"),
            &None,
            TsFileExportResult::Possible(vec!["xxx".to_string()]),
        );
        assert_export_result(
            &["export * from 'xxx';"].join("\n"),
            &Some("MyComponent".to_string()),
            TsFileExportResult::Possible(vec!["xxx".to_string()]),
        );
        assert_export_result(
            &["export * from 'aaa';", "export * from 'bbb';"].join("\n"),
            &Some("MyComponent".to_string()),
            TsFileExportResult::Possible(vec!["aaa".to_string(), "bbb".to_string()]),
        );
    }
}
