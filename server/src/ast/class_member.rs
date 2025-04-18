use html_languageservice::html_data::Description;
use swc_common::{comments::Comments, source_map::SmallPos, BytePos};
use swc_ecma_ast::{ClassMember, ClassProp};
use tower_lsp::lsp_types::{MarkupContent, MarkupKind};

use crate::renderer::{multi_threaded_comment::MultiThreadedComments, RenderCachePropType};

use super::{
    comment::get_markdown,
    decorator::is_specified_decorator,
    get_class_prop_pos, get_decorator_prop_params,
    prop_name::{get_name_form_prop_name, get_name_span_from_prop_name},
};

pub fn _filter_specified_prop<'a>(prop: &'a ClassMember, name: &str) -> Option<&'a ClassProp> {
    if let ClassMember::ClassProp(prop) = prop {
        for decorator in &prop.decorators {
            if is_specified_decorator(decorator, name) {
                return Some(prop);
            }
        }
        return None;
    }
    None
}

pub fn filter_all_prop_method(prop: &ClassMember) -> bool {
    match prop {
        ClassMember::PrivateProp(_)
        | ClassMember::ClassProp(_)
        | ClassMember::Method(_)
        | ClassMember::PrivateMethod(_) => true,
        _ => false,
    }
}

pub fn get_class_member_name(member: &ClassMember) -> String {
    match member {
        ClassMember::PrivateProp(prop) => prop.key.name.to_string(),
        ClassMember::ClassProp(prop) => get_name_form_prop_name(&prop.key),
        ClassMember::Method(method) => get_name_form_prop_name(&method.key),
        ClassMember::PrivateMethod(method) => method.key.name.to_string(),
        _ => String::new(),
    }
}

fn get_class_member_type(member: &ClassMember, source: &str) -> String {
    match member {
        ClassMember::PrivateProp(prop) => {
            if let Some(type_ann) = &prop.type_ann {
                let span = type_ann.span;
                source[span.lo.to_usize()..span.hi.to_usize()].to_string()
            } else {
                String::new()
            }
        }
        ClassMember::ClassProp(prop) => {
            if let Some(type_ann) = &prop.type_ann {
                let span = type_ann.span;
                source[span.lo.to_usize()..span.hi.to_usize()].to_string()
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

pub fn get_class_member_description(
    member: &ClassMember,
    comments: &MultiThreadedComments,
    class_name: &str,
    source: &str,
) -> Option<Description> {
    let comments = comments
        .get_leading(get_class_member_pos(member))
        .unwrap_or_default();
    let desc = comments
        .iter()
        .map(get_markdown)
        .collect::<Vec<String>>()
        .join("\n");
    let member_type = get_class_member_type(member, source);
    if member_type.len() == 0 {
        return None;
    }
    let mut value = format!(
        "```typescript\n(property) {}.{}{}\n```\n",
        class_name,
        get_class_member_name(member),
        member_type,
    );
    if desc.len() > 0 {
        value += &format!("\n{}", desc);
    }
    Some(Description::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    }))
}

pub fn get_class_member_pos(member: &ClassMember) -> BytePos {
    match member {
        ClassMember::ClassProp(prop) => get_class_prop_pos(prop),
        ClassMember::PrivateProp(prop) => {
            let mut pos = prop.span.lo;
            let decorators = &prop.decorators;
            if decorators.len() > 0 {
                pos = decorators[0].span.lo;
            }
            pos
        }
        _ => BytePos(0),
    }
}

pub fn get_class_member_name_pos(member: &ClassMember) -> BytePos {
    match member {
        ClassMember::ClassProp(prop) => get_name_span_from_prop_name(&prop.key).lo,
        ClassMember::PrivateProp(prop) => prop.key.span.lo,
        _ => BytePos(0),
    }
}

pub fn get_class_member_prop_type(member: &ClassMember) -> RenderCachePropType {
    match member {
        ClassMember::Method(_) => RenderCachePropType::Method,
        ClassMember::PrivateMethod(_) => RenderCachePropType::Method,
        _ => RenderCachePropType::Property,
    }
}

/// 获取属性参数
/// 返回: (typ, default, required)
pub fn get_class_member_prop_params(
    member: &ClassMember,
    source: &str,
) -> Option<(Option<String>, bool, bool)> {
    match member {
        ClassMember::ClassProp(prop) => {
            if prop.decorators.len() == 1 {
                let decorator = &prop.decorators[0];
                get_decorator_prop_params(decorator, source)
            } else {
                None
            }
        }
        ClassMember::PrivateProp(prop) => {
            if prop.decorators.len() == 1 {
                let decorator = &prop.decorators[0];
                get_decorator_prop_params(decorator, source)
            } else {
                None
            }
        }
        _ => None,
    }
}
