use html_languageservice::html_data::Description;
use std::{collections::HashMap, fs, path::PathBuf, str::FromStr};
use tower_lsp::lsp_types::Location;

use lsp_textdocument::FullTextDocument;
use swc_common::source_map::SmallPos;
use swc_ecma_ast::{ClassMember, Decl, Expr, ModuleDecl, ModuleItem, Stmt};
use tower_lsp::lsp_types::{Range, Uri};

use crate::ast;

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

/// 解析组件库
/// 从 types/index.d.ts 文件中解析组件库
/// 如果遇到导入语句，那么先进入导入语句的文件
/// 将中间结果保存到上下文
/// 获取继承自 Vue 的组件
pub async fn _parse_lib(uri: &Uri) -> Vec<LibComponent> {
    // 尝试解析 uri 下 types/index.d.ts 文件
    // 如果遇到 export * from './xxx'，那么递归解析
    // 获取继承自 Vue 的组件的 ClassExpr
    let components = vec![];
    let mut file_path = PathBuf::from_str(&uri.path().to_string()).unwrap();
    file_path.push("types/index.d.ts");
    if file_path.is_file() {
        let _idx_map = _parse_file(file_path);
    }
    components
}

/// 递归解析路径指向的文件获取当前文件导出的所有定义
fn _parse_file(path: PathBuf) -> HashMap<Option<String>, Decl> {
    // 获取文件内容
    let mut idx_map = HashMap::new();
    let mut local_idx_map = HashMap::new();
    let source = fs::read_to_string(path).unwrap();
    let module = ast::parse_source(&source, 0, source.len()).0.unwrap();
    for item in module.body {
        match item {
            ModuleItem::ModuleDecl(module) => match module {
                ModuleDecl::Import(_import_decl) => {
                    // TODO: 导入声明需要再从对应文件获取导出
                }
                ModuleDecl::ExportDecl(export_decl) => {
                    idx_map.insert(
                        Some(ast::get_ident_from_export_decl(&export_decl)),
                        export_decl.decl,
                    );
                }
                ModuleDecl::ExportNamed(_named_export) => {
                    // TODO: 如果存在 src ，那么先从对应文件获取导出
                }
                ModuleDecl::ExportDefaultDecl(export_default_decl) => {
                    idx_map.insert(
                        None,
                        ast::_convert_default_decl_to_decl(export_default_decl.decl),
                    );
                }
                ModuleDecl::ExportDefaultExpr(_export_default_expr) => {
                    // TODO: 导出默认表达式，如果是标识符，需要先从本地声明中获取
                }
                ModuleDecl::ExportAll(_export_all) => {
                    // TODO: 从导入文件获取全部导出
                }
                ModuleDecl::TsImportEquals(_ts_import_equals_decl) => {}
                ModuleDecl::TsExportAssignment(_ts_export_assignment) => {}
                ModuleDecl::TsNamespaceExport(_ts_namespace_export_decl) => {}
            },
            ModuleItem::Stmt(stmt) => {
                if let Stmt::Decl(decl) = stmt {
                    local_idx_map.insert(ast::get_ident_from_decl(&decl), decl);
                }
            }
        }
    }
    idx_map
}

/// 解析特定格式的 UI 库，作为临时的代替方案
/// 假设 UI 库满足下面的要求
/// * 组件全部位于 types 目录下
/// * 存在 types/index.d.ts 文件
/// * 如果遍历 types 目录时是一个文件，那么取其中的 class 作为组件
/// * 如果遍历 types 目录时是一个目录，那么存在静态属性的文件是主组件其他组件挂载到该组件下
pub fn parse_specific_lib(uri: &Uri) -> LibRenderCache {
    let mut components = vec![];
    let mut file_path = PathBuf::from_str(&uri.path().to_string()).unwrap();
    let name = file_path
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or(uri.path().to_string());
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
                                            uri: Uri::from_str(&format!(
                                                "file://{}",
                                                path.to_string_lossy()
                                            ))
                                            .unwrap(),
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
                        uri: Uri::from_str(&format!("file://{}", path.to_string_lossy())).unwrap(),
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
