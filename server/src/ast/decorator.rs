use swc_ecma_ast::{Callee, Decorator, Expr, ExprOrSpread};

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
