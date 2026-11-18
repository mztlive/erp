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
    /// 业务命令解析附件字段时，将本批次已登记的临时引用就地改写为正式资产 ID，并记入已消费集合。
    /// 完成替换返回 `true`，既有正式资产 ID 原样保留并返回 `false`；未登记或重复消费的临时引用返回错误。
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool>;

    /// 业务命令解析完全部附件字段后，对比已消费引用与批次输入；有漏用或未知引用时拒绝继续落库。
    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()>;

    /// 检查正式资产 ID 是否由本批次分配，区分本次上传与调用方引用的既有文件。
    fn contains_id(&self, id: &FileAssetId) -> bool;

    /// 材料校验可用本批次已捕获的敏感级别；ID 不在批次内时返回 `None`，不从其他资产推断。
    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass>;

    /// 沿用业务命令指定的执行器保存已准备的资产元数据，使引用与文件登记可在同一事务完成；传播唯一键冲突或写入错误。
    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()>;

    /// 生成幂等命令指纹时，应使用真实内容清单，不绑定每次上传分配的资产 ID 或存储键。
    /// 默认空清单仅兼容旧消费者；新上传实现须提供清单，非空批次的消费者须拒绝空清单。
    fn content_manifest(&self) -> Vec<PendingFileContent> {
        Vec::new()
    }

    /// 判断是否有本次待登记的文件，供业务命令区分纯既有引用与新增上传。
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
