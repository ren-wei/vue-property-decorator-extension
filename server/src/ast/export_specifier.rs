use swc_ecma_ast::ExportSpecifier;

/// 获取导出表示，第一层 Option 为 None, 则值无效
pub fn get_export_from_export_specifier(specifier: &ExportSpecifier) -> Option<Option<String>> {
    match specifier {
        ExportSpecifier::Default(specifier) => Some(Some(specifier.exported.to_string())),
        ExportSpecifier::Named(specifier) => {
            if let Some(exported) = &specifier.exported {
                let exported = exported.atom().to_string();
                if &exported == "default" {
                    Some(None)
                } else {
                    Some(Some(exported))
                }
            } else {
                Some(Some(specifier.orig.atom().to_string()))
            }
        }
        ExportSpecifier::Namespace(_) => None,
    }
}

/// 获取原始表示，第一层 Option 为 None ，则值无效
pub fn get_orig_name_from_export_specifier(specifier: &ExportSpecifier) -> Option<Option<String>> {
    match specifier {
        ExportSpecifier::Default(_) => Some(None),
        ExportSpecifier::Named(specifier) => {
            let orig = specifier.orig.atom().to_string();
            if orig == "default" {
                Some(None)
            } else {
                Some(Some(orig))
            }
        }
        ExportSpecifier::Namespace(_) => None,
    }
}
