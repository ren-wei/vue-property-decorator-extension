use html_languageservice::html_data::Description;
use swc_common::{comments::Comments, BytePos};
use swc_ecma_ast::{ClassMember, ClassProp};
use tower_lsp::lsp_types::{MarkupContent, MarkupKind};

use crate::renderer::multi_threaded_comment::MultiThreadedComments;

use super::{
    comment::get_markdown, decorator::is_specified_decorator, get_class_prop_pos,
    prop_name::get_name_form_prop_name,
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

pub fn get_class_member_name(prop: &ClassMember) -> String {
    match prop {
        ClassMember::PrivateProp(prop) => prop.key.name.to_string(),
        ClassMember::ClassProp(prop) => get_name_form_prop_name(&prop.key),
        ClassMember::Method(method) => get_name_form_prop_name(&method.key),
        ClassMember::PrivateMethod(method) => method.key.name.to_string(),
        _ => String::new(),
    }
}

pub fn get_class_member_description(
    member: &ClassMember,
    comments: &MultiThreadedComments,
) -> Option<Description> {
    let comments = comments.get_leading(get_class_member_pos(member))?;
    Some(Description::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: comments
            .iter()
            .map(get_markdown)
            .collect::<Vec<String>>()
            .join("\n"),
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
