//! 套餐图生成端口与 P0 兜底实现。

use super::limits::PACKAGE_IMAGE_FALLBACK_VERSION;
use erp_core::{Error, Result};

/// 套餐图生成端口。调用方只依赖本端口，不判断当前是兜底还是正式实现。
pub trait PackageImageGenerator: Send + Sync {
    /// 根据成员有序图片 URL 生成套餐主图 URL。
    ///
    /// # 参数
    /// * `member_image_urls` - 与套餐内 SKU 顺序一致；无图成员占位为空。
    ///
    /// # 返回
    /// 返回一条套餐主图 URL。
    ///
    /// # 错误
    /// 无法产出有效封面时返回错误；该套餐不得写入有效陈列。
    fn generate(&self, member_image_urls: &[Option<String>]) -> Result<String>;

    /// 返回生成实现版本，写入长期事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回实现版本字符串。
    ///
    /// # 错误
    /// 无。
    fn implementation_version(&self) -> &'static str;
}

/// P0 兜底：按成员顺序取第一张非空图片 URL。
#[derive(Debug, Default, Clone, Copy)]
pub struct FirstNonEmptyMemberImage;

impl PackageImageGenerator for FirstNonEmptyMemberImage {
    /// 从左到右扫描成员 URL，跳过空值和只含空白的值。
    ///
    /// # 参数
    /// * `member_image_urls` - 有序成员图片
    ///
    /// # 返回
    /// 命中的第一张非空 URL。
    ///
    /// # 错误
    /// 全部成员都没有可用 URL 时失败，不得用占位图或空白 URL 冒充已生成。
    fn generate(&self, member_image_urls: &[Option<String>]) -> Result<String> {
        first_non_empty_url(member_image_urls)
            .map(ToOwned::to_owned)
            .ok_or_else(|| Error::from("套餐成员均无可用图片，不能生成套餐主图"))
    }

    /// 返回 P0 兜底实现版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `p0-first-member-v1`。
    ///
    /// # 错误
    /// 无。
    fn implementation_version(&self) -> &'static str {
        PACKAGE_IMAGE_FALLBACK_VERSION
    }
}

/// 扫描第一张非空成员图。
///
/// # 参数
/// * `member_image_urls` - 有序成员图片
///
/// # 返回
/// 命中时返回 URL 切片；否则 `None`。
///
/// # 错误
/// 无。
pub fn first_non_empty_url(member_image_urls: &[Option<String>]) -> Option<&str> {
    member_image_urls
        .iter()
        .find_map(|url| url.as_deref().map(str::trim).filter(|value| !value.is_empty()))
}

/// 套餐主图长期引用。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageCoverRef {
    /// 文件资产身份。
    pub file_asset_id: String,
    /// 内容校验信息（快照时的内容指纹）。
    pub content_checksum: String,
    /// 对象存储键快照，供公开预览在原资产变更后仍能读取。
    pub storage_object_key: String,
    /// 生成实现版本。
    pub generator_version: String,
}

impl PackageCoverRef {
    /// 由端口输出的成员图引用登记套餐主图。
    ///
    /// # 参数
    /// * `file_asset_id` - 命中成员的快照资产身份
    /// * `content_checksum` - 内容指纹
    /// * `storage_object_key` - 快照对象键
    /// * `generator_version` - 端口实现版本
    ///
    /// # 返回
    /// 返回独立保存的套餐主图引用。
    ///
    /// # 错误
    /// 资产身份或指纹为空时拒绝。
    pub fn from_port_output(
        file_asset_id: String,
        content_checksum: String,
        storage_object_key: String,
        generator_version: String,
    ) -> Result<Self> {
        let file_asset_id = file_asset_id.trim();
        let content_checksum = content_checksum.trim();
        if file_asset_id.is_empty() || content_checksum.is_empty() {
            return Err(Error::from("套餐主图必须登记有效的文件资产引用"));
        }
        Ok(Self {
            file_asset_id: file_asset_id.to_string(),
            content_checksum: content_checksum.to_string(),
            storage_object_key: storage_object_key.trim().to_string(),
            generator_version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{first_non_empty_url, FirstNonEmptyMemberImage, PackageImageGenerator};

    #[test]
    fn fallback_takes_first_non_blank_member() {
        let urls = [
            None,
            Some("  ".into()),
            Some("asset://one".into()),
            Some("asset://two".into()),
        ];
        let generator = FirstNonEmptyMemberImage;
        assert_eq!(generator.generate(&urls).unwrap(), "asset://one");
        assert_eq!(first_non_empty_url(&urls), Some("asset://one"));
        assert_eq!(generator.implementation_version(), "p0-first-member-v1");
    }

    #[test]
    fn fallback_rejects_all_empty_members() {
        let urls = [None, Some("".into()), Some(" \n".into())];
        assert!(FirstNonEmptyMemberImage.generate(&urls).is_err());
    }
}
