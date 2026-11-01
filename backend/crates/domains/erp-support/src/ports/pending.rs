//! 业务命令可持久化的已准备附件批次消费方端口。

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use crate::entity::file_asset::{PendingFileReferenceSet, SensitivityClass};
use crate::error::Result;

/// 重传稳定的真实附件内容清单，不包含每次上传生成的资产身份与存储键。
#[derive(Debug, Clone, Serialize)]
pub struct PendingFileContent {
    /// multipart 临时引用。
    pub reference: String,
    /// 文件展示名。
    pub file_name: String,
    /// 已校验的 MIME。
    pub content_type: String,
    /// 真实内容大小。
    pub byte_size: u64,
    /// 服务端计算的真实内容 HMAC。
    pub content_hmac: String,
}

/// 业务事务开始前已构造文件的事实与持久化钩子。
///
/// 业务领域只接收附件 ID、敏感级别和恰好消费一次的检查，
/// 不得接收 `FileAsset` 实体、S3 客户端或审计聚合。
#[async_trait]
pub trait PendingAttachmentBatch: Send + Sync {
    /// 把本批次中的临时文件引用替换为正式资产 ID。
    ///
    /// # 参数
    /// * `id` - 待解析的文件 ID；命中临时引用时就地改写
    /// * `used` - 本命令已经消费的临时引用
    ///
    /// # 返回
    /// 发生替换时返回 `true`；正式 ID 保持不变并返回 `false`。
    ///
    /// # 错误
    /// 引用了未登记文件，或同一临时文件被重复消费时返回错误。
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool>;

    /// 拒绝业务命令没有消费完的已上传文件。
    ///
    /// # 参数
    /// * `used` - 本命令已经消费的临时引用
    ///
    /// # 返回
    /// 已上传文件恰好全部被消费时无返回值。
    ///
    /// # 错误
    /// 存在未消费引用或出现未知引用时返回错误。
    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()>;

    /// 判断 `id` 是否属于本已准备批次。
    ///
    /// # 参数
    /// * `id` - 正式文件资产 ID
    ///
    /// # 返回
    /// 属于本批次时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    fn contains_id(&self, id: &FileAssetId) -> bool;

    /// 返回准备本批次时为 `id` 捕获的敏感级别。
    ///
    /// # 参数
    /// * `id` - 正式文件资产 ID
    ///
    /// # 返回
    /// 已捕获时返回敏感级别；`id` 不属于本批次时返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass>;

    /// 在调用方选定的执行器上持久化已准备的文件元数据。
    ///
    /// # 参数
    /// * `db` - 目标数据库
    /// * `executor` - 调用方选定的执行器
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// 唯一键冲突或底层写入失败时返回错误。
    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()>;

    /// 返回用于幂等命令指纹的真实内容清单；非空批次的消费者必须拒绝缺少清单。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 默认返回空清单以保留旧消费者接口；新上传实现应覆盖。
    ///
    /// # 错误
    /// 不返回错误。
    fn content_manifest(&self) -> Vec<PendingFileContent> {
        Vec::new()
    }

    /// 判断本批次是否准备了待持久化的文件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 没有待持久化文件时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    fn is_empty(&self) -> bool;
}

/// 不登记新文件的单域命令使用的空批次。
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyPendingAttachments;

#[async_trait]
impl PendingAttachmentBatch for EmptyPendingAttachments {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        PendingFileReferenceSet::default().resolve_id(id, used)
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        PendingFileReferenceSet::default().ensure_all_used(used)
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        PendingFileReferenceSet::default().contains_id(id)
    }

    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass> {
        PendingFileReferenceSet::default().sensitivity(id)
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

    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass> {
        (**self).sensitivity(id)
    }

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        (**self).persist(db, executor).await
    }

    fn content_manifest(&self) -> Vec<PendingFileContent> {
        (**self).content_manifest()
    }

    fn is_empty(&self) -> bool {
        (**self).is_empty()
    }
}
