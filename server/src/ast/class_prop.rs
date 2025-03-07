use swc_common::{comments::Comments, BytePos};
use swc_ecma_ast::{ClassProp, Expr, Lit, PropName};

use crate::renderer::multi_threaded_comment::MultiThreadedComments;

use super::{
    comment::get_markdown, expr_is_true, get_decorator_args, get_object_props,
    get_value_of_specified_prop, is_specified_decorator,
};

pub fn get_class_prop_pos(class_prop: &ClassProp) -> BytePos {
    let mut pos = class_prop.span.lo;
    let decorators = &class_prop.decorators;
    if decorators.len() > 0 {
        pos = decorators[0].span.lo;
    }
    pos
}

pub fn get_class_prop_name(class_prop: &ClassProp) -> String {
    match &class_prop.key {
        PropName::Ident(name) => name.sym.to_string(),
        PropName::Str(name) => name.value.to_string(),
        PropName::Num(name) => name.value.to_string(),
        PropName::BigInt(name) => name.value.to_string(),
        PropName::Computed(_name) => "computed".to_string(),
    }
}

pub fn get_class_prop_description(
    class_prop: &ClassProp,
    comments: &MultiThreadedComments,
) -> String {
    let comments = comments.get_leading(get_class_prop_pos(class_prop));
    if let Some(comments) = comments {
        comments
            .iter()
            .map(get_markdown)
            .collect::<Vec<String>>()
            .join("\n")
    } else {
        "".to_string()
    }
}

pub fn get_vue_prop_default(class_prop: &ClassProp, decorator_name: &str, index: usize) -> bool {
    for decorator in &class_prop.decorators {
        if is_specified_decorator(decorator, decorator_name) {
            if let Some(args) = get_decorator_args(decorator) {
                if index < args.len() {
                    if let Some(props) = get_object_props(&args[index].expr) {
                        for prop in props {
                            if get_value_of_specified_prop(prop, "default").is_some() {
                                return true;
                            }
                        }
                    }
                }
            }
            break;
        }
    }
    false
}

pub fn get_vue_prop_required(class_prop: &ClassProp, decorator_name: &str, index: usize) -> bool {
    for decorator in &class_prop.decorators {
        if is_specified_decorator(decorator, decorator_name) {
            if let Some(args) = get_decorator_args(decorator) {
                if args.len() == 1 && index < args.len() {
                    if let Some(props) = get_object_props(&args[index].expr) {
                        for prop in props {
                            if let Some(expr) = get_value_of_specified_prop(prop, "required") {
                                return expr_is_true(expr);
                            }
                        }
                    }
                }
            }
            break;
        }
    }
    false
}

pub fn get_vue_prop_event(class_prop: &ClassProp) -> Option<String> {
    for decorator in &class_prop.decorators {
        if is_specified_decorator(decorator, "Model") {
            if let Some(args) = get_decorator_args(decorator) {
                let expr = &args[0].expr;
                if let Expr::Lit(Lit::Str(event)) = expr.as_ref() {
                    return Some(event.value.to_string());
                }
            }
            break;
        }
    }
    None
}
