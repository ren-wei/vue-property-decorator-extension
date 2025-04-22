use html_languageservice::html_data::Description;
use std::{fs, path::PathBuf};
use tower_lsp::lsp_types::Location;

use lsp_textdocument::FullTextDocument;
use swc_common::source_map::SmallPos;
use swc_ecma_ast::{ClassMember, Decl, Expr, ModuleDecl, ModuleItem};
use tower_lsp::lsp_types::{Range, Uri};

use crate::{ast, util};

#[derive(Debug)]
pub struct LibRenderCache {
    pub name: String,
    pub components: Vec<LibComponent>,
}

#[derive(Debug)]
pub struct LibComponent {
    pub name: String,
    pub name_location: Location,
    pub description: Option<Description>,
    /// 在组件上挂载的静态属性组件
    pub static_props: Vec<Box<LibComponent>>,
    /// 定义的属性，包括继承的属性，不包括方法
    pub props: Vec<LibComponentProp>,
}

#[derive(Debug)]
pub struct LibComponentProp {
    pub name: String,
    pub location: Location,
}

/// 解析特定格式的 UI 库，作为临时的代替方案
/// 假设 UI 库满足下面的要求
/// * 组件全部位于 types 目录下
/// * 存在 types/index.d.ts 文件
/// * 如果遍历 types 目录时是一个文件，那么取其中的 class 作为组件
/// * 如果遍历 types 目录时是一个目录，那么存在静态属性的文件是主组件其他组件挂载到该组件下
pub fn parse_specific_lib(uri: &Uri) -> LibRenderCache {
    let mut components = vec![];
    let mut file_path = util::to_file_path(uri);
    let name = file_path
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or(util::to_file_path_string(uri));
    file_path.push("types/index.d.ts");
    if file_path.is_file() {
        file_path.pop();
        if let Ok(entries) = fs::read_dir(file_path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_dir() {
                        // 如果是目录，那么解析目录下的文件
                        if let Ok(entries) = fs::read_dir(path) {
                            for entry in entries {
                                if let Ok(entry) = entry {
                                    let path = entry.path();
                                    if path.is_file() {
                                        let result = parse_specific_file(&path);
                                        if let Some(result) = result {
                                            components.push(result);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        let result = parse_specific_file(&path);
                        if let Some(result) = result {
                            components.push(result);
                        }
                    }
                }
            }
        }
    }

    // TODO: 获取继承的属性

    let mut result = vec![];
    for c in components {
        result.push(c.0);
    }
    LibRenderCache {
        name,
        components: result,
    }
}

/// 解析特定格式的组件文件
/// 假设满足以下条件
/// * 组件导出为命名的 class
/// * 一个文件最多只定义了一个组件
/// * 继承的组件的标识符不变
/// 返回值：（component, extends_component)
fn parse_specific_file(path: &PathBuf) -> Option<(LibComponent, Option<String>)> {
    let source = fs::read_to_string(path).unwrap();
    let (module, comments) = ast::parse_source(&source, 0, source.len());
    for item in module.unwrap().body {
        if let ModuleItem::ModuleDecl(module) = item {
            if let ModuleDecl::ExportDecl(decl) = module {
                if let Decl::Class(class_decl) = decl.decl {
                    let class = &class_decl.class;
                    // 继承组件
                    let mut super_component = None;
                    if let Some(super_class) = &class.super_class {
                        if let Expr::Ident(ident) = super_class.as_ref() {
                            let ident = ident.sym.to_string();
                            if &ident != "Vue" {
                                super_component = Some(ident);
                            }
                        }
                    }
                    let document = FullTextDocument::new("typescript".to_string(), 0, source);
                    // 获取属性
                    let mut props = vec![];
                    let static_props = vec![];
                    for member in &class.body {
                        if let ClassMember::ClassProp(prop) = member {
                            if prop.is_static {
                                // TODO: 静态属性
                            } else {
                                let name = ast::get_class_prop_name(&prop);
                                if name.len() > 0 {
                                    props.push(LibComponentProp {
                                        name,
                                        location: Location {
                                            uri: util::create_uri_from_path(&path),
                                            range: Range {
                                                start: document.position_at(prop.span.lo.to_u32()),
                                                end: document.position_at(prop.span.hi.to_u32()),
                                            },
                                        },
                                    });
                                }
                            }
                        }
                    }
                    let name_location = Location {
                        uri: util::create_uri_from_path(&path),
                        range: Range::new(
                            document.position_at(class.span.lo.to_u32()),
                            document.position_at(class.span.hi.to_u32()),
                        ),
                    };
                    return Some((
                        LibComponent {
                            name: class_decl.ident.sym.to_string(),
                            name_location,
                            description: ast::get_class_decl_description(&class_decl, &comments),
                            static_props,
                            props,
                        },
                        super_component,
                    ));
                }
            }
        }
    }
    None
}
