//! Consumer port for qualification attachment existence and sensitivity.

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use persistence_core::Executor;

use crate::error::Result;

/// 资质附件的最小文件事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssetFact {
    /// 文件资产稳定 ID。
    pub id: String,
    /// 敏感级别稳定代码。
    pub sensitivity_class: String,
}

/// 供应商读取资质附件存在性的消费方端口。
///
/// 文件资产实体与集合仍由 `erp-support` 拥有。
#[async_trait]
pub trait FileAssetFactsPort: Send + Sync {
    /// 按文件 ID 读取未删除附件事实。
    ///
    /// # 参数
    /// * `attachment_id` - 文件资产 ID
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 返回
    /// 附件不存在或已删除时返回 `None`。
    ///
    /// # 错误
    /// 文件资产查询失败时返回仓储或映射错误。
    async fn find_by_id(
        &self,
        attachment_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>>;
}

/// Empty attachment lookup used by isolated supplier unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyFileAssetFacts;

#[async_trait]
impl FileAssetFactsPort for EmptyFileAssetFacts {
    async fn find_by_id(
        &self,
        _attachment_id: &FileAssetId,
        _executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>> {
        Ok(None)
    }
}
