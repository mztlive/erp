//! 商品命令可持久化的已准备附件批次消费端口。

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use mongodb::Database;
use persistence_core::Executor;

use crate::error::Result;

/// Facts and persist hook for files constructed before a catalog business transaction.
///
/// Catalog receives attachment IDs and consume-once checks. It must not receive
/// `FileAsset` entities, S3 clients or audit aggregates. Outer `PendingFileAssets`
/// preparation stays in `erp-processes`.
#[async_trait]
pub trait PendingAttachmentBatch: Send + Sync {
    /// 把临时文件引用替换为本批次的正式资产 ID。
    ///
    /// # 参数
    /// * `id` - 待解析的文件 ID；属于本批次临时引用时就地改为正式 ID
    /// * `used` - 本命令已消费的临时引用集合
    ///
    /// # 返回
    /// 完成临时引用替换时返回 `true`；该 ID 不是临时引用时返回 `false`。
    ///
    /// # 错误
    /// 临时引用无效或不能消费时返回 `erp_core::Error`。
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool>;

    /// 拒绝业务命令未消费的已上传文件。
    ///
    /// # 参数
    /// * `used` - 命令实际消费的临时引用
    ///
    /// # 返回
    /// 没有遗留文件时无返回值。
    ///
    /// # 错误
    /// 存在未消费的上传文件时返回 `erp_core::Error`。
    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()>;

    /// 判断正式资产 ID 是否属于本批次。
    ///
    /// # 参数
    /// * `id` - 正式资产 ID
    ///
    /// # 返回
    /// 属于本批次时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    fn contains_id(&self, id: &FileAssetId) -> bool;

    /// 在调用方选定的执行器上持久化已准备的文件元数据。
    ///
    /// # 参数
    /// * `db` - 目标数据库
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 写入完成时无返回值。
    ///
    /// # 错误
    /// 持久化失败时返回商品领域错误。
    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()>;

    /// 判断本批次是否没有待持久化的文件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 没有待写入文件时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    fn is_empty(&self) -> bool;
}

/// Empty batch used by single-domain commands that do not register new files.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyPendingAttachments;

#[async_trait]
impl PendingAttachmentBatch for EmptyPendingAttachments {
    fn resolve_id(&self, id: &mut FileAssetId, _used: &mut HashSet<String>) -> erp_core::Result<bool> {
        let value = id.as_ref().trim();
        if !value.starts_with("pending-file:") {
            return Ok(false);
        }
        let token = value.strip_prefix("pending-file:").unwrap_or("");
        if token.trim().is_empty() {
            return Err(erp_core::Error::from("临时文件引用 token 不能为空"));
        }
        Err(erp_core::Error::from("业务命令引用了未上传的文件"))
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        if used.is_empty() {
            Ok(())
        } else {
            Err(erp_core::Error::from("存在未被业务命令引用的上传文件"))
        }
    }

    fn contains_id(&self, _id: &FileAssetId) -> bool {
        false
    }

    async fn persist(&self, _db: &Database, _executor: &mut dyn Executor) -> Result<()> {
        Ok(())
    }

    fn is_empty(&self) -> bool {
        true
    }
}

#[async_trait]
impl PendingAttachmentBatch for Arc<dyn PendingAttachmentBatch> {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        (**self).resolve_id(id, used)
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        (**self).ensure_all_used(used)
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        (**self).contains_id(id)
    }

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        (**self).persist(db, executor).await
    }

    fn is_empty(&self) -> bool {
        (**self).is_empty()
    }
}
