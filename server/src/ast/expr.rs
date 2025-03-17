use swc_ecma_ast::{Expr, Lit, PropOrSpread};

pub fn get_object_props(expr: &Expr) -> Option<&Vec<PropOrSpread>> {
    if let Expr::Object(expr) = expr {
        return Some(&expr.props);
    }
    None
}

pub fn _expr_is_true(expr: &Expr) -> bool {
    if let Expr::Lit(Lit::Bool(expr)) = expr {
        expr.value
    } else {
        false
    }
}
