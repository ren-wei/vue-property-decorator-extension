#[cfg(target_os = "windows")]
use std::str::FromStr;
use std::{collections::HashMap, path::PathBuf};

use tower_lsp::lsp_types::Uri;

use crate::util;

/// # 解析别名
/// 从 tsconfig.json 文件内容获取别名信息
pub fn parse_alias(tsconfig: &str, root_uri: &Uri) -> HashMap<String, String> {
    let root_path = util::to_file_path(root_uri);
    let mut alias = HashMap::new();
    let tsconfig = serde_json::from_str::<serde_json::Value>(&tsconfig);
    if let Ok(tsconfig) = tsconfig {
        if let Some(compiler_options) = tsconfig.get("compilerOptions") {
            if let Some(paths) = compiler_options.get("paths") {
                if let Some(paths) = paths.as_object() {
                    for (key, value) in paths {
                        if key.ends_with("/*") && value.is_array() {
                            let key = key[..key.len() - 1].to_string();
                            if let Some(value) = value.as_array() {
                                if value.len() == 1 {
                                    if let Some(value) = value[0].as_str() {
                                        if value.ends_with("/*") {
                                            alias.insert(
                                                key,
                                                format!(
                                                    "{}/{}",
                                                    root_path.to_string_lossy(),
                                                    &value[..value.len() - 1]
                                                ),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    alias
}

/// # 导入路径解析为绝对路径
///
/// * 处理别名
/// * 处理相对路径
///
/// ## 注意
/// 不判断对应文件是否存在
/// 不添加后缀
pub fn parse_import_path(
    base_uri: &Uri,
    path: &str,
    alias: &HashMap<String, String>,
    root_uri: &Uri,
) -> PathBuf {
    if path.starts_with(".") {
        // 处理相对路径
        let base_path = util::to_file_path(base_uri);
        // 获取基础路径的父目录
        let mut result = match base_path.parent() {
            Some(parent) => parent.to_path_buf(),
            None => PathBuf::new(),
        };

        // 按路径分隔符分割相对路径
        let file_path = path.to_string();
        for part in file_path.split("/") {
            match part {
                // 如果是 "."，表示当前目录，不做处理，继续下一个部分
                "." => continue,
                // 如果是 ".."，表示上级目录，移除结果路径的最后一个组件
                ".." => {
                    if !result.pop() {
                        // 如果结果路径为空，不能再向上级目录移动，直接返回空路径
                        return PathBuf::new();
                    }
                }
                // 其他情况，将该部分添加到结果路径中
                _ => {
                    let inner = result.as_mut_os_string();
                    let buf = inner.as_encoded_bytes();
                    let need_sep = buf.last().map(|c| *c != b'/').unwrap_or(false);
                    if need_sep {
                        inner.push("/");
                    }
                    inner.push(part);
                }
            }
        }
        return result;
    }
    // 处理别名
    let mut file_path = path.to_string().replace("//", "/");
    for (key, value) in alias {
        if path.starts_with(key) {
            file_path = file_path.replace(key, value);
            return PathBuf::from(file_path);
        }
    }
    // 可能位于 node_modules 中
    let path = util::to_file_path(root_uri).join("node_modules").join(path);
    #[cfg(target_os = "windows")]
    {
        PathBuf::from_str(&path.to_string_lossy().replace("\\", "/")).unwrap()
    }
    #[cfg(not(target_os = "windows"))]
    {
        path
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, str::FromStr};

    use tower_lsp::lsp_types::Uri;

    use super::{parse_alias, parse_import_path};

    fn assert_alias(tsconfig: &str, expected: &[(&str, &str)]) {
        let root_uri = Uri::from_str("file:///tmp/project").unwrap();
        let alias = parse_alias(tsconfig, &root_uri);
        let expected =
            HashMap::from_iter(expected.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        assert_eq!(alias, expected);
    }

    fn assert_parse(path: &str, expected: &str, alias: &[(&str, &str)]) {
        let base_uri = Uri::from_str("file:///tmp/project/base.vue").unwrap();
        let root_uri = Uri::from_str("file:///tmp/project").unwrap();
        let result = parse_import_path(
            &base_uri,
            path,
            &HashMap::from_iter(alias.iter().map(|(k, v)| (k.to_string(), v.to_string()))),
            &root_uri,
        );
        assert_eq!(result.to_string_lossy(), expected);
    }

    #[test]
    fn test_parse_alias() {
        assert_alias(
            r#"{
			"compilerOptions": {
				"target": "esnext",
				"paths": {
					"@global/*": ["src/com/core/*"],
					"@workspace/*": ["src/com/module/business/workspace/*"],
					"@api/*": ["src/com/api/*"],
					"@components/*": ["src/com/components/*"]
				}
			}
		}"#,
            &[
                ("@global/", "/tmp/project/src/com/core/"),
                (
                    "@workspace/",
                    "/tmp/project/src/com/module/business/workspace/",
                ),
                ("@api/", "/tmp/project/src/com/api/"),
                ("@components/", "/tmp/project/src/com/components/"),
            ],
        );
    }

    #[test]
    fn alias() {
        assert_parse(
            "@api/metadata",
            "/tmp/project/api/metadata",
            &[("@api/", "/tmp/project/api/")],
        );
        assert_parse(
            "@api/path//to",
            "/tmp/project/api/path/to",
            &[("@api/", "/tmp/project/api/")],
        );
    }

    #[test]
    fn relative_path() {
        assert_parse("./other.vue", "/tmp/project/other.vue", &[]);
        assert_parse("../../tmq/project/other.vue", "/tmq/project/other.vue", &[]);
    }

    #[test]
    fn node_modules() {
        assert_parse("vue", "/tmp/project/node_modules/vue", &[]);
    }
}
