//! 审批材料的不可变文件引用；存储完整指纹，公开 DTO 必须显式筛选字段。

use erp_core::ids::FileAssetId;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 提交时冻结的文件元数据；不得保存对象存储键或临时下载 URL。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalMaterialFile {
    pub file_asset_id: FileAssetId,
    pub file_name: String,
    pub content_type: String,
    pub byte_size: u64,
    pub asset_version: u64,
    pub content_hmac: String,
}

impl ApprovalMaterialFile {
    /// 校验冻结材料的有界元数据与内容指纹。
    ///
    /// # 参数
    /// 无；读取自身冻结字段。
    /// # 返回
    /// 材料可用于版本绑定时成功。
    /// # 错误
    /// 空白身份、超长元数据或非法内容指纹时拒绝。
    pub fn validate(&self) -> Result<()> {
        let id: &str = self.file_asset_id.as_ref();
        if id.is_empty()
            || id.trim() != id
            || id.len() > 128
            || self.file_name.trim().is_empty()
            || self.file_name.chars().count() > 256
            || self.content_type.trim().is_empty()
            || self.content_type.len() > 128
            || self.content_hmac.len() != 64
            || !self.content_hmac.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Error::from("审批材料元数据无效"));
        }
        Ok(())
    }

    /// 比较当前文件事实是否仍是审批提交时冻结的同一内容。
    ///
    /// # 参数
    /// * `current` - 文件领域当前读取并投影的元数据。
    /// # 返回
    /// 身份、版本、类型、大小和内容指纹全部一致时返回 true。
    /// # 错误
    /// 无；无效或变化的事实返回 false。
    pub fn matches_current(&self, current: &Self) -> bool {
        self.validate().is_ok() && current.validate().is_ok() && self == current
    }
}
