//! 业务命令上传文件的临时引用值对象。

use std::collections::{HashMap, HashSet};

use crate::errors::{Error, Result};
use crate::file_asset::SensitivityClass;
use crate::ids::FileAssetId;

/// multipart 业务命令中临时文件引用的稳定前缀。
pub const PENDING_FILE_REFERENCE_PREFIX: &str = "pending-file:";

/// 已验证的临时文件引用。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PendingFileReference(String);

impl PendingFileReference {
    /// 解析临时文件引用并校验非空 token。
    ///
    /// # 参数
    /// * `value` - 待解析的完整临时引用
    ///
    /// # 返回
    /// 返回去除首尾空白后的强类型引用。
    ///
    /// # 错误
    /// 前缀错误或前缀后的 token 为空时返回领域校验错误。
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        let Some(token) = value.strip_prefix(PENDING_FILE_REFERENCE_PREFIX) else {
            return Err(Error::from("临时文件引用格式无效"));
        };
        if token.trim().is_empty() {
            return Err(Error::from("临时文件引用 token 不能为空"));
        }
        Ok(Self(value.to_string()))
    }

    /// 当文件 ID 使用临时引用前缀时解析该引用。
    ///
    /// # 参数
    /// * `id` - 业务 DTO 中的文件资产 ID
    ///
    /// # 返回
    /// 普通正式 ID 返回 `Ok(None)`；临时引用返回已验证值对象。
    ///
    /// # 错误
    /// 临时引用使用固定前缀但 token 非法时返回领域校验错误。
    pub fn from_file_asset_id(id: &FileAssetId) -> Result<Option<Self>> {
        let value = id.as_ref();
        if !value.trim().starts_with(PENDING_FILE_REFERENCE_PREFIX) {
            return Ok(None);
        }
        Self::parse(value).map(Some)
    }

    /// 返回稳定字符串表示。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingFileTarget {
    id: FileAssetId,
}

/// 单次业务命令登记的临时文件引用集合。
#[derive(Debug, Clone, Default)]
pub struct PendingFileReferenceSet {
    targets: HashMap<PendingFileReference, PendingFileTarget>,
    sensitivity_by_id: HashMap<String, SensitivityClass>,
}

impl PendingFileReferenceSet {
    /// 由 Service 已生成的正式身份构造引用集合。
    ///
    /// # 参数
    /// * `entries` - 临时引用、正式资产 ID 与敏感级别三元组
    ///
    /// # 返回
    /// 返回可执行解析和完整消费校验的集合。
    ///
    /// # 错误
    /// 同一临时引用或正式资产 ID 重复登记时返回领域校验错误。
    pub fn new(entries: Vec<(PendingFileReference, FileAssetId, SensitivityClass)>) -> Result<Self> {
        let mut targets = HashMap::with_capacity(entries.len());
        let mut sensitivity_by_id = HashMap::with_capacity(entries.len());
        for (reference, id, sensitivity) in entries {
            if targets.contains_key(&reference) {
                return Err(Error::from("临时文件引用不能重复"));
            }
            if sensitivity_by_id.insert(id.to_string(), sensitivity).is_some() {
                return Err(Error::from("正式文件资产 ID 不能重复"));
            }
            targets.insert(reference, PendingFileTarget { id });
        }
        Ok(Self {
            targets,
            sensitivity_by_id,
        })
    }

    /// 把临时引用替换为正式资产 ID，并登记一次消费。
    ///
    /// # 参数
    /// * `id` - 待解析的业务文件 ID
    /// * `used` - 本命令已经消费的临时引用集合
    ///
    /// # 返回
    /// 发生临时引用替换时返回 `true`，普通正式 ID 返回 `false`。
    ///
    /// # 错误
    /// 引用了未上传文件，或同一临时文件被重复消费时返回领域校验错误。
    pub fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> Result<bool> {
        let Some(reference) = PendingFileReference::from_file_asset_id(id)? else {
            return Ok(false);
        };
        let target = self
            .targets
            .get(&reference)
            .ok_or_else(|| Error::from("业务命令引用了未上传的文件"))?;
        if !used.insert(reference.as_str().to_string()) {
            return Err(Error::from("同一临时文件不能重复消费"));
        }
        *id = target.id.clone();
        Ok(true)
    }

    /// 校验全部已上传文件恰好被业务命令消费一次。
    ///
    /// # 错误
    /// 存在未消费引用或调用方传入未知引用时返回领域校验错误。
    pub fn ensure_all_used(&self, used: &HashSet<String>) -> Result<()> {
        if used.len() != self.targets.len()
            || self
                .targets
                .keys()
                .any(|reference| !used.contains(reference.as_str()))
        {
            return Err(Error::from("存在未被业务命令引用的上传文件"));
        }
        Ok(())
    }

    /// 判断正式资产 ID 是否属于本集合。
    pub fn contains_id(&self, id: &FileAssetId) -> bool {
        self.sensitivity_by_id.contains_key(id.as_ref())
    }

    /// 返回本集合中正式资产 ID 的敏感级别。
    pub fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass> {
        self.sensitivity_by_id.get(id.as_ref()).copied()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{PendingFileReference, PendingFileReferenceSet};
    use crate::file_asset::SensitivityClass;
    use crate::ids::FileAssetId;

    fn references() -> PendingFileReferenceSet {
        PendingFileReferenceSet::new(vec![
            (
                PendingFileReference::parse(" pending-file:one ").unwrap(),
                FileAssetId::new("asset-1"),
                SensitivityClass::Sensitive,
            ),
            (
                PendingFileReference::parse("pending-file:two").unwrap(),
                FileAssetId::new("asset-2"),
                SensitivityClass::General,
            ),
        ])
        .unwrap()
    }

    #[test]
    fn reference_rejects_wrong_prefix_and_empty_token() {
        assert!(PendingFileReference::parse("asset-1").is_err());
        assert!(PendingFileReference::parse("pending-file:").is_err());
        assert!(PendingFileReference::parse("pending-file:   ").is_err());
    }

    #[test]
    fn set_rejects_duplicate_reference() {
        let reference = PendingFileReference::parse("pending-file:one").unwrap();
        assert!(PendingFileReferenceSet::new(vec![
            (
                reference.clone(),
                FileAssetId::new("asset-1"),
                SensitivityClass::General
            ),
            (
                reference,
                FileAssetId::new("asset-2"),
                SensitivityClass::Sensitive
            ),
        ])
        .is_err());
    }

    #[test]
    fn resolve_tracks_unknown_duplicate_and_full_consumption() {
        let references = references();
        let mut used = HashSet::new();
        let mut first = FileAssetId::new("pending-file:one");
        references.resolve_id(&mut first, &mut used).unwrap();
        assert_eq!(first.as_ref(), "asset-1");
        assert!(references.ensure_all_used(&used).is_err());

        let mut duplicate = FileAssetId::new("pending-file:one");
        assert!(references.resolve_id(&mut duplicate, &mut used).is_err());
        let mut unknown = FileAssetId::new("pending-file:missing");
        assert!(references.resolve_id(&mut unknown, &mut used).is_err());

        let mut second = FileAssetId::new("pending-file:two");
        references.resolve_id(&mut second, &mut used).unwrap();
        references.ensure_all_used(&used).unwrap();
        assert_eq!(references.sensitivity(&first), Some(SensitivityClass::Sensitive));
    }
}
