use std::path::{Component, Path};

use crate::{Error, Result};

/// 校验存储路径为不越过存储根的非空相对路径。
pub(super) fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(Error::PathError("存储路径必须是非空相对路径".to_string()));
    }

    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::PathError("存储路径不能越过基础目录".to_string()));
            }
        }
    }

    if !has_normal_component {
        return Err(Error::PathError("存储路径不能为空".to_string()));
    }

    Ok(())
}

/// 将已校验的相对路径转换为跨平台 S3 对象键。
pub(super) fn object_key_path(path: &Path) -> Result<String> {
    validate_relative_path(path)?;

    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(
                value
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| Error::PathError("存储路径必须是 UTF-8".to_string()))
                    .and_then(|component| {
                        if component.contains('\\') {
                            return Err(Error::PathError("存储路径不能包含反斜杠".to_string()));
                        }
                        Ok(component)
                    }),
            ),
            Component::CurDir => None,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => unreachable!(),
        })
        .collect::<Result<Vec<_>>>()
        .map(|components| components.join("/"))
}
