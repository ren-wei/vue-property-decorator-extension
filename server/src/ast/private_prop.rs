use swc_ecma_ast::PrivateProp;

pub fn get_private_prop_name(prop: &PrivateProp) -> String {
    prop.key.id.sym.to_string()
}
