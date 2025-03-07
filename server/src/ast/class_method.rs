use swc_ecma_ast::ClassMethod;

pub fn get_class_method_name(method: &ClassMethod) -> String {
    super::get_name_from_prop_name(&method.key)
}
