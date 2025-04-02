use swc_common::{source_map::SmallPos, Spanned};
use swc_ecma_ast::{Callee, Decorator, Expr, ExprOrSpread, Lit, Prop, PropOrSpread};

use super::prop_name::get_name_form_prop_name;

pub fn is_specified_decorator(decorator: &Decorator, name: &str) -> bool {
    match decorator.expr.as_ref() {
        Expr::Call(expr) => match &expr.callee {
            Callee::Expr(expr) => match expr.as_ref() {
                Expr::Ident(ident) => ident.sym.to_string() == name,
                _ => false,
            },
            _ => false,
        },
        Expr::Ident(ident) => &ident.sym.to_string() == name,
        _ => false,
    }
}

pub fn get_decorator_args(decorator: &Decorator) -> Option<&Vec<ExprOrSpread>> {
    if let Expr::Call(expr) = decorator.expr.as_ref() {
        return Some(&expr.args);
    }
    None
}

pub fn get_decorator_prop_params(
    decorator: &Decorator,
    source: &str,
) -> Option<(Option<String>, bool, bool)> {
    if is_specified_decorator(decorator, "Prop") {
        let args = get_decorator_args(decorator)?;
        if args.len() == 1 {
            let arg = &args[0];
            if let Expr::Object(obj) = &arg.expr.as_ref() {
                let mut typ = None;
                let mut default = false;
                let mut required = false;
                for prop in &obj.props {
                    if let PropOrSpread::Prop(prop) = prop {
                        if let Prop::KeyValue(prop) = prop.as_ref() {
                            let key = get_name_form_prop_name(&prop.key);
                            if key == "type" {
                                typ = Some(
                                    source[prop.value.span().lo.to_usize()
                                        ..prop.value.span().hi.to_usize()]
                                        .to_string(),
                                )
                            } else if key == "default" {
                                default = true;
                            } else if key == "required" {
                                if let Expr::Lit(Lit::Bool(value)) = &prop.value.as_ref() {
                                    required = value.value;
                                }
                            }
                        }
                    }
                }
                return Some((typ, default, required));
            }
        }
    }
    None
}
