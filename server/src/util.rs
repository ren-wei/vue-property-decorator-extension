use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use percent_encoding::{self, AsciiSet, CONTROLS};
use tower_lsp::lsp_types::Uri;

const ASCII_SET: &AsciiSet = &CONTROLS.add(b' ').add(b'$').add(b':');

pub fn create_uri_from_path(path: &Path) -> Uri {
    let path = path.to_string_lossy();
    create_uri_from_str(&path.to_string())
}

pub fn create_uri_from_str(path: &str) -> Uri {
    let add_prefix = path.contains(":") && !path.starts_with("/");
    let path = path.replace("\\", "/");
    let path = percent_encoding::percent_encode(path.as_bytes(), ASCII_SET).to_string();
    if add_prefix {
        Uri::from_str(&format!("file:///{}", path)).unwrap()
    } else {
        Uri::from_str(&format!("file://{}", path)).unwrap()
    }
}

pub fn to_file_path(uri: &Uri) -> PathBuf {
    let path = uri.path().to_string();
    let path = percent_encoding::percent_decode_str(&path)
        .decode_utf8_lossy()
        .to_string();
    if path.contains(":") && path.starts_with("/") {
        return PathBuf::from_str(&path[1..]).unwrap();
    }
    PathBuf::from_str(&path).unwrap()
}

pub fn to_file_path_string(uri: &Uri) -> String {
    to_file_path(uri).to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use tower_lsp::lsp_types::Uri;

    use crate::util::to_file_path;

    use super::create_uri_from_str;

    fn assert_uri(path: &str, uri: &str) {
        assert_eq!(create_uri_from_str(path), Uri::from_str(uri).unwrap());
    }

    fn assert_path(uri: &str, path: &str) {
        assert_eq!(
            to_file_path(&Uri::from_str(uri).unwrap()),
            PathBuf::from_str(path).unwrap()
        );
    }

    #[test]
    fn unix_link() {
        assert_uri("/home/user/file.md", "file:///home/user/file.md");
        assert_uri("/home/user/file .md", "file:///home/user/file%20.md");
        assert_path("file:///home/user/file.md", "/home/user/file.md");
        assert_path("file:///home/user/file%20.md", "/home/user/file .md");
    }

    #[test]
    fn windows() {
        assert_path("file:///d%3A/code/project", "d:/code/project");
        assert_uri("d:/code/project", "file:///d%3A/code/project");
    }
}
