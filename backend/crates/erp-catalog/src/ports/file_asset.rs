//! 商品媒体与品牌 Logo 文件存在性的消费端口。

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use persistence_core::Executor;

use crate::error::Result;

/// Minimal file-asset existence fact consumed by catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssetFact {
    /// File asset stable id.
    pub id: String,
}

/// Catalog reads media/logo existence through this consumer port.
///
/// File-asset entities and collections remain owned by `erp-support`.
#[async_trait]
pub trait FileAssetFactsPort: Send + Sync {
    /// 按 ID 读取未删除的文件资产事实。
    ///
    /// # 参数
    /// * `asset_id` - 文件资产 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 资产存在且未删除时返回事实；不存在或已删除时返回 `None`。
    ///
    /// # 错误
    /// 读取失败时返回下层错误。
    async fn find_by_id(
        &self,
        asset_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>>;

    /// 返回尚未登记的文件资产 ID，并保持输入顺序。
    ///
    /// 空输入返回空向量。重复的缺失 ID 只出现一次。
    ///
    /// # 参数
    /// * `asset_ids` - 待核对的文件资产 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回尚未登记的 ID。
    ///
    /// # 错误
    /// 核对失败时返回下层错误。
    async fn missing_ids(
        &self,
        asset_ids: &[FileAssetId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FileAssetId>>;
}

/// Empty attachment lookup used by isolated catalog unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyFileAssetFacts;

#[async_trait]
impl FileAssetFactsPort for EmptyFileAssetFacts {
    async fn find_by_id(
        &self,
        _asset_id: &FileAssetId,
        _executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>> {
        Ok(None)
    }

    async fn missing_ids(
        &self,
        asset_ids: &[FileAssetId],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<FileAssetId>> {
        let mut missing = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in asset_ids {
            if seen.insert(id.to_string()) {
                missing.push(id.clone());
            }
        }
        Ok(missing)
    }
}
