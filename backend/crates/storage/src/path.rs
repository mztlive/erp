use std::path::{Component, Path};

use crate::{Error, Result};

/// 将相对路径转换为跨平台 S3 对象键。
pub(super) fn object_key_path(path: &Path) -> Result<String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(Error::PathError("存储路径必须是非空相对路径".to_string()));
    }

    let mut key = String::new();
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
                if !key.is_empty() {
                    key.push('/');
                }
                key.push_str(text);
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

    Ok(key)
}

/// 空或带首尾空白的配置值一律拒绝，与上传键约束同口径。
pub(super) fn is_blank_or_padded(value: &str) -> bool {
    value.trim().is_empty() || value.trim() != value
}

/// 将可选对象键前缀规范为不带首尾分隔符的相对键。
pub(super) fn normalize_prefix(prefix: Option<String>) -> Result<Option<String>> {
    let Some(prefix) = prefix else {
        return Ok(None);
    };
    if is_blank_or_padded(&prefix)
        || prefix.starts_with('/')
        || prefix.ends_with('/')
        || prefix.contains('\\')
        || prefix.split('/').any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(Error::InvalidConfig("S3 key_prefix 必须是不带首尾分隔符的相对对象键前缀".to_string()));
    }
    Ok(Some(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 相对路径按分量以 `/` 连接为对象键。
    #[test]
    fn joins_normal_segments_with_slash() -> Result<()> {
        assert_eq!(object_key_path(Path::new("images/example.png"))?, "images/example.png");
        assert_eq!(object_key_path(Path::new("a/./b"))?, "a/b");
        Ok(())
    }

    /// 绝对路径与空路径拒绝为对象键。
    #[test]
    fn rejects_absolute_or_empty_path() {
        assert!(matches!(object_key_path(Path::new("/a.png")), Err(Error::PathError(_))));
        assert!(matches!(object_key_path(Path::new("")), Err(Error::PathError(_))));
        assert!(matches!(object_key_path(Path::new(".")), Err(Error::PathError(_))));
    }

    /// 父目录分量不得越过基础目录。
    #[test]
    fn rejects_parent_directory() {
        assert!(matches!(object_key_path(Path::new("../escaped.txt")), Err(Error::PathError(_))));
        assert!(matches!(object_key_path(Path::new("a/../../b")), Err(Error::PathError(_))));
    }

    /// 反斜杠分量拒绝为对象键。
    #[test]
    fn rejects_backslash_segment() {
        assert!(matches!(object_key_path(Path::new("a\\b")), Err(Error::PathError(_))));
    }

    /// 前缀首尾分隔符与保留分量拒绝为规范前缀。
    #[test]
    fn rejects_unsafe_prefix() -> Result<()> {
        assert!(normalize_prefix(None)?.is_none());
        assert_eq!(
            normalize_prefix(Some("tenant-a/uploads".to_string()))?.as_deref(),
            Some("tenant-a/uploads")
        );
        assert!(matches!(normalize_prefix(Some("/tenant-a".to_string())), Err(Error::InvalidConfig(_))));
        assert!(matches!(
            normalize_prefix(Some("tenant-a/../escape".to_string())),
            Err(Error::InvalidConfig(_))
        ));
        Ok(())
    }
}
