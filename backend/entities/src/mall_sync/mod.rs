//! 域 D23 `mall_sync`：mall_sales_sync_job、mall_sales_sync_cursor、mall_sales_order_snapshot、
//! mall_sales_reconciliation_job(+_item)、master_mapping_task（页面：W17）。
//!
//! 字段字典与唯一约束见数据模型 §6.13（第一期商城卡券销售单同步），
//! 一期快照应用不变量见 §8.4 第 2 条，公共字段归属按 §4.3 判定：
//! - `mall_sales_sync_job` / `mall_sales_reconciliation_job(+_item)` /
//!   `master_mapping_task` 是作业与差异任务，按 §6.13 字典精确建模（状态、
//!   计数、统计字段各自建模，不硬套 StableBase）；
//! - `mall_sales_sync_cursor` 是同步水位指针（每个来源商城一个），水位只前进，
//!   提供 `move_forward()` 单调推进，不允许通用 update 回退；
//! - `mall_sales_order_snapshot` 是历史快照记录，内容创建后不可修改，
//!   只允许按固定状态机推进 `mapping_status`（`update` 受限）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 common 基元。
//! 作业状态、版本、增量时间范围、快照顺序、映射注册表与命令幂等身份由本域
//! 类型确定；分页全部安全持久化后再前移水位等跨聚合写入顺序，仍由 P3 服务层
//! 在事务中编排（§8.4 第 2 条）。

use sha2::{Digest, Sha256};

pub mod external_order_key;
pub mod mall_sales_order_snapshot;
pub mod mall_sales_reconciliation;
pub mod mall_sales_sync_cursor;
pub mod mall_sales_sync_job;
pub mod master_mapping_task;
pub mod reapply_operation;

pub use external_order_key::ExternalOrderKey;
pub use mall_sales_order_snapshot::{
    MallSalesOrderSnapshot, MallSalesOrderSnapshotData, SnapshotMappingStatus,
};
pub use mall_sales_reconciliation::{
    MallSalesReconciliationItem, MallSalesReconciliationItemData, MallSalesReconciliationJob,
    MallSalesReconciliationJobData, ReconciliationDifferenceType, ReconciliationItemStatus,
    ReconciliationJobStatus,
};
pub use mall_sales_sync_cursor::MallSalesSyncCursor;
pub use mall_sales_sync_job::{
    MallSalesSyncJob, MallSalesSyncJobData, MallSalesSyncJobStatus, MallSalesSyncJobType, MallSyncTimeRange,
    MallSyncTriggerSource, SyncJobCompletionDisposition,
};
pub use master_mapping_task::{
    MappingSourceIdentity, MappingTaskStatus, MappingTaskType, MasterMappingTask, MasterMappingTaskData,
};
pub use reapply_operation::{
    MallSnapshotReapplyOperation, MallSnapshotReapplyOperationData, ReapplyOperationStatus,
};

/// W17 命令的稳定幂等身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MallSyncCommandIdentity {
    audit_id: String,
    fingerprint: String,
    idempotency_key_hash: String,
}

impl MallSyncCommandIdentity {
    /// 从稳定命令身份与序列化载荷构造不可逆审计身份。
    ///
    /// # 参数
    /// * `prefix` - 审计 ID 固定前缀
    /// * `actor_id` - 操作人
    /// * `action` - 稳定动作名
    /// * `resource_id` - 资源 ID
    /// * `idempotency_key` - 客户端幂等键
    /// * `payload` - 完整命令序列化字节
    ///
    /// # 返回
    /// 返回审计 ID、命令指纹与幂等键摘要。
    pub fn new(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_id: &str,
        idempotency_key: &str,
        payload: &[u8],
    ) -> Self {
        let audit_digest =
            sha256_hex(format!("{actor_id}|{action}|{resource_id}|{}", idempotency_key.trim()).as_bytes());
        Self {
            audit_id: format!("{prefix}{audit_digest}"),
            fingerprint: sha256_hex(payload),
            idempotency_key_hash: sha256_hex(idempotency_key.trim().as_bytes()),
        }
    }

    /// 返回稳定审计 ID。
    ///
    /// # 返回
    /// 返回不含原始幂等键的审计收据 ID。
    pub fn audit_id(&self) -> &str {
        &self.audit_id
    }

    /// 返回完整命令指纹。
    ///
    /// # 返回
    /// 返回用于拒绝同键异参的 SHA-256 指纹。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 返回客户端幂等键摘要。
    ///
    /// # 返回
    /// 返回不暴露原始幂等键的 SHA-256 摘要。
    pub fn idempotency_key_hash(&self) -> &str {
        &self.idempotency_key_hash
    }

    /// 派生同步作业稳定 ID。
    ///
    /// # 返回
    /// 返回由审计 ID 派生的 40 位摘要作业 ID。
    pub fn sync_job_id(&self) -> String {
        let digest = sha256_hex(format!("sync-job|{}", self.audit_id).as_bytes());
        format!("w17-job-{}", &digest[..40])
    }

    /// 派生映射谱系与目标稳定 ID。
    ///
    /// # 返回
    /// 返回由命令指纹派生的 `(mapping_id, target_id)`。
    pub fn mapping_lineage_ids(&self) -> (String, String) {
        let map_digest = sha256_hex(format!("map|{}", self.fingerprint).as_bytes());
        let target_digest = sha256_hex(format!("target|{}", self.fingerprint).as_bytes());
        (
            format!("w17-map-{}", &map_digest[..40]),
            format!("w17-target-{}", &target_digest[..40]),
        )
    }
}

fn sha256_hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    MallSalesOrderSnapshotId, MallSalesReconciliationItemId, MallSalesReconciliationJobId,
    MallSalesSyncCursorId, MallSalesSyncJobId, MasterMappingTaskId, SalesOrderId, SalesOrderRevisionId,
    SourceSystemId,
};

#[cfg(test)]
mod command_identity_tests {
    use super::MallSyncCommandIdentity;

    #[test]
    fn identity_hides_key_and_derives_stable_domain_ids() {
        let identity = MallSyncCommandIdentity::new(
            "w17-command-",
            "actor-1",
            "confirm",
            "mapping-1",
            "raw-secret-key",
            br#"{"decision":"CONFIRM"}"#,
        );
        let same = MallSyncCommandIdentity::new(
            "w17-command-",
            "actor-1",
            "confirm",
            "mapping-1",
            "raw-secret-key",
            br#"{"decision":"CONFIRM"}"#,
        );

        assert_eq!(identity, same);
        assert!(!identity.audit_id().contains("raw-secret-key"));
        assert_eq!(identity.fingerprint().len(), 64);
        assert_eq!(identity.idempotency_key_hash().len(), 64);
        assert_eq!(identity.sync_job_id(), same.sync_job_id());
        assert_eq!(identity.mapping_lineage_ids(), same.mapping_lineage_ids());
    }
}
