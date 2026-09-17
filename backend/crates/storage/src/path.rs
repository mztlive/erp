use std::path::{Component, Path};

use crate::{Error, Result};

/// 单次遍历切分相对路径为对象键分量。
fn path_segments(path: &Path) -> Result<Vec<String>> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(Error::PathError("存储路径必须是非空相对路径".to_string()));
    }

    let mut segments = Vec::new();
    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                has_normal_component = true;
                let text =
                    value.to_str().ok_or_else(|| Error::PathError("存储路径必须是 UTF-8".to_string()))?;
                if text.contains('\\') {
                    return Err(Error::PathError("存储路径不能包含反斜杠".to_string()));
                }
                segments.push(text.to_owned());
            },
            Component::CurDir => {},
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::PathError("存储路径不能越过基础目录".to_string()));
            },
        }
    }

    if !has_normal_component {
        return Err(Error::PathError("存储路径不能为空".to_string()));
    }

    Ok(segments)
}

/// 将相对路径转换为跨平台 S3 对象键。
pub(super) fn object_key_path(path: &Path) -> Result<String> {
    path_segments(path).map(|segments| segments.join("/"))
}
