use swc_ecma_ast::ImportSpecifier;

/// 获取当前表示
pub fn get_local_from_import_specifier(specifier: &ImportSpecifier) -> String {
    specifier.local().sym.to_string()
}

/// 获取原始导出，如果是默认，返回 None
/// 第一层 Option 如果是 None，那么表示无结果
pub fn get_orig_name_from_import_specifier(specifier: &ImportSpecifier) -> Option<Option<String>> {
    match specifier {
        ImportSpecifier::Default(_) => Some(None),
        ImportSpecifier::Named(specifier) => {
            if let Some(imported) = &specifier.imported {
                Some(Some(imported.atom().to_string()))
            } else {
                Some(Some(specifier.local.sym.to_string()))
            }
        }
        ImportSpecifier::Namespace(_) => None,
    }
}
