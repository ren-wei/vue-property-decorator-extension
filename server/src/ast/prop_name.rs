use swc_common::Span;
use swc_ecma_ast::PropName;

pub fn get_name_form_prop_name(prop_name: &PropName) -> String {
    match prop_name {
        PropName::BigInt(_) => "unknown".to_string(),
        PropName::Computed(_) => "unknown".to_string(),
        PropName::Ident(name) => name.sym.to_string(),
        PropName::Num(name) => name.value.to_string(),
        PropName::Str(name) => name.value.to_string(),
    }
}

pub fn get_name_span_from_prop_name(prop_name: &PropName) -> Span {
    match prop_name {
        PropName::BigInt(name) => name.span,
        PropName::Computed(name) => name.span,
        PropName::Ident(name) => name.span,
        PropName::Num(name) => name.span,
        PropName::Str(name) => name.span,
    }
}
