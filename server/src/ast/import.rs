use swc_ecma_ast::{ImportDecl, ImportSpecifier};

pub fn get_specified_import<'a>(
    imports: &'a Vec<&ImportDecl>,
    name: &str,
) -> Option<(&'a ImportSpecifier, &'a str)> {
    for import in imports {
        for specifier in &import.specifiers {
            match specifier {
                ImportSpecifier::Default(default) => {
                    if default.local.sym.as_str() == name {
                        return Some((specifier, import.src.value.as_str()));
                    }
                }
                ImportSpecifier::Named(named) => {
                    if named.local.sym.as_str() == name {
                        return Some((specifier, import.src.value.as_str()));
                    }
                }
                ImportSpecifier::Namespace(_) => {}
            }
        }
    }
    None
}
