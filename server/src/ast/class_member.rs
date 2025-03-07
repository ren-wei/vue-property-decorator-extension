use swc_ecma_ast::{ClassMember, ClassProp};

use super::{decorator::is_specified_decorator, prop_name::get_name_form_prop_name};

pub fn filter_specified_prop<'a>(prop: &'a ClassMember, name: &str) -> Option<&'a ClassProp> {
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
