use swc_ecma_ast::{Expr, Prop, PropName, PropOrSpread};

pub fn get_value_of_specified_prop<'a>(prop: &'a PropOrSpread, key: &str) -> Option<&'a Expr> {
    if let PropOrSpread::Prop(prop) = prop {
        if let Prop::KeyValue(prop) = prop.as_ref() {
            if let PropName::Ident(ident) = &prop.key {
                if ident.sym.to_string() == key {
                    return Some(prop.value.as_ref());
                }
            }
        }
    }
    None
}
