use swc_ecma_ast::{ClassDecl, Decl, DefaultDecl, FnDecl};

pub fn get_ident_from_decl(decl: &Decl) -> String {
    match &decl {
        swc_ecma_ast::Decl::Class(class_decl) => class_decl.ident.to_string(),
        swc_ecma_ast::Decl::Fn(fn_decl) => fn_decl.ident.to_string(),
        swc_ecma_ast::Decl::Var(_) => String::new(),
        swc_ecma_ast::Decl::Using(_) => String::new(),
        swc_ecma_ast::Decl::TsInterface(ts_interface_decl) => ts_interface_decl.id.to_string(),
        swc_ecma_ast::Decl::TsTypeAlias(ts_type_alias_decl) => ts_type_alias_decl.id.to_string(),
        swc_ecma_ast::Decl::TsEnum(ts_enum_decl) => ts_enum_decl.id.to_string(),
        swc_ecma_ast::Decl::TsModule(_) => String::new(),
    }
}

pub fn convert_default_decl_to_decl(default_decl: DefaultDecl) -> Decl {
    match default_decl {
        DefaultDecl::Class(class_expr) => Decl::Class(ClassDecl {
            ident: class_expr.ident.unwrap_or_default(),
            declare: false,
            class: class_expr.class,
        }),
        DefaultDecl::Fn(fn_expr) => Decl::Fn(FnDecl {
            ident: fn_expr.ident.unwrap_or_default(),
            declare: false,
            function: fn_expr.function,
        }),
        DefaultDecl::TsInterfaceDecl(ts_interface_decl) => Decl::TsInterface(ts_interface_decl),
    }
}
