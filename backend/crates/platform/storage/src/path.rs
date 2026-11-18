use std::path::{Component, Path};

use crate::{Error, Result};

// 对象读写和公开 URL 共用同一相对路径规则：忽略 `.`，其余正常分量以 `/` 连接。
// 空路径、绝对路径、父目录、根目录、Windows 前缀、非 UTF-8 或含反斜杠的分量均返回 PathError，
// 避免同一输入在不同平台生成不同对象键或越过配置前缀。
pub(super) fn object_key_path(path: &Path) -> Result<String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(Error::PathError("存储路径必须是非空相对路径".to_string()));
    }

    let mut key = String::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
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

    if key.is_empty() {
        return Err(Error::PathError("存储路径不能为空".to_string()));
    }
    Ok(key)
}

// 对配置值拒绝全空白和首尾空白，而不是悄悄修剪，避免改变实际使用的 bucket、凭据或键前缀。
pub(super) fn is_blank_or_padded(value: &str) -> bool {
    value.trim().is_empty() || value.trim() != value
}

// 创建存储客户端时校验可选的对象键前缀，合法值原样用于 S3 键和公开 URL。
// 拒绝空字符串、全空白、首尾空白或斜杠，以及反斜杠、空段、`.` 和 `..`；未配置前缀时直接使用相对对象键。
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

    // 检查普通相对路径及含当前目录分量的路径都生成用 `/` 分隔的对象键。
    #[test]
    fn joins_normal_segments_with_slash() -> Result<()> {
        assert_eq!(object_key_path(Path::new("images/example.png"))?, "images/example.png");
        assert_eq!(object_key_path(Path::new("a/./b"))?, "a/b");
        Ok(())
    }

    // 验证绝对路径以及没有有效分量的空串或当前目录不能成为对象键。
    #[test]
    fn rejects_absolute_or_empty_path() {
        assert!(matches!(object_key_path(Path::new("/a.png")), Err(Error::PathError(_))));
        assert!(matches!(object_key_path(Path::new("")), Err(Error::PathError(_))));
        assert!(matches!(object_key_path(Path::new(".")), Err(Error::PathError(_))));
        assert!(matches!(object_key_path(Path::new("././")), Err(Error::PathError(_))));
    }

    // 验证无论父目录出现在开头还是正常目录之后，都不能借此越过配置的存储前缀。
    #[test]
    fn rejects_parent_directory() {
        assert!(matches!(object_key_path(Path::new("../escaped.txt")), Err(Error::PathError(_))));
        assert!(matches!(object_key_path(Path::new("a/../../b")), Err(Error::PathError(_))));
    }

    // 验证含反斜杠的路径不能生成对象键，防止同一输入在不同平台被解释成不同路径。
    #[test]
    fn rejects_backslash_segment() {
        assert!(matches!(object_key_path(Path::new("a\\b")), Err(Error::PathError(_))));
    }

    // 确认缺省与合法多段前缀能通过校验，绝对前缀和含父目录的前缀会在建客户端时被拒绝。
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
