//! 域 D22 `legacy_import`：legacy_import_batch、legacy_import_row、legacy_import_confirmation（页面：W18）。
//!
//! 字段字典与唯一约束见数据模型 §6.12（旧数据导入兼容层），导入失败处理见
//! §11.5，公共字段归属按 §4.3 判定：
//! - `legacy_import_batch` / `legacy_import_row` 是导入兼容层的批次与行记录，
//!   只使用 `BaseModel` 持久化元数据，状态与统计字段按 §6.12 各自建模，
//!   不硬套 StableBase；
//! - `legacy_import_confirmation` 是正式确认事实（§6.12），不设业务软删除，
//!   状态字段（`PENDING`/`CONFIRMED`/`REJECTED`/`INVALIDATED`）按 §6.12
//!   实现固定状态机（数据模型第 7 章，禁止运行时扩展）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 common 基元。
//! 确认矩阵、版本、行状态与命令幂等身份由本域类型确定；`work_item` 联动、
//! 多仓储写入与事务边界仍由 P3 服务层编排。

use sha2::{Digest, Sha256};

pub mod apply_result_set;
pub mod confirmation_factories;
pub mod import_row_factory;
pub mod legacy_import_batch;
pub mod legacy_import_confirmation;
pub mod legacy_import_row;

pub use apply_result_set::{ApplyResultDraft, ApplyResultItem, ApplyResultOutcome, ApplyResultSet};
pub use confirmation_factories::{confirmation_work_item, confirmation_workflow_action};
pub use import_row_factory::{build_import_rows, ImportRowSpec};
pub use legacy_import_batch::{LegacyImportBatch, LegacyImportBatchData, LegacyImportBatchStatus};
pub use legacy_import_confirmation::{
    ConfirmationDecision, ConfirmationMatrixDecision, ConfirmationScope, ConfirmationStatus,
    LegacyImportConfirmation, LegacyImportConfirmationData,
};
pub use legacy_import_row::{ImportStatus, LegacyImportRow, LegacyImportRowData, MappingStatus, ParseStatus};

/// W18 导入强命令的稳定幂等身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImportCommandIdentity {
    audit_id: String,
    fingerprint: String,
}

impl LegacyImportCommandIdentity {
    /// 从命令身份字段与规范化载荷片段构造不可逆收据身份。
    ///
    /// 审计主键只保存幂等键参与计算后的摘要；载荷指纹对每个字段加长度前缀，
    /// 避免简单拼接产生歧义并拒绝同键异参。
    ///
    /// # 参数
    /// * `prefix` - 审计 ID 固定前缀
    /// * `actor_id` - 命令操作人
    /// * `action` - 稳定动作名
    /// * `resource_id` - 命令资源 ID
    /// * `idempotency_key` - 客户端幂等键
    /// * `parts` - 已规范化的完整命令字段序列
    ///
    /// # 返回
    /// 返回不暴露原始幂等键的审计 ID 与命令指纹。
    pub fn new(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_id: &str,
        idempotency_key: &str,
        parts: &[&str],
    ) -> Self {
        let audit_id = format!(
            "{prefix}{}",
            sha256_hex(format!("{actor_id}|{action}|{resource_id}|{idempotency_key}").as_bytes())
        );
        let mut digest = Sha256::new();
        for part in parts {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part.as_bytes());
        }
        Self {
            audit_id,
            fingerprint: encode_digest(digest.finalize()),
        }
    }

    /// 返回稳定审计收据 ID。
    ///
    /// # 返回
    /// 返回不含原始幂等键的 SHA-256 派生 ID。
    pub fn audit_id(&self) -> &str {
        &self.audit_id
    }

    /// 返回完整命令指纹。
    ///
    /// # 返回
    /// 返回用于拒绝同键异参的长度前缀 SHA-256 指纹。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// 计算稳定 SHA-256 十六进制文本。
///
/// # 参数
/// * `value` - 待摘要字节
///
/// # 返回
/// 返回 64 位小写十六进制摘要。
fn sha256_hex(value: &[u8]) -> String {
    encode_digest(Sha256::digest(value))
}

/// 将摘要字节编码为小写十六进制文本。
///
/// # 参数
/// * `digest` - SHA-256 摘要字节
///
/// # 返回
/// 返回 64 位小写十六进制摘要。
fn encode_digest(digest: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = digest.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    ExternalIdentityMapId, FileAssetId, LegacyImportBatchId, LegacyImportConfirmationId, LegacyImportRowId,
    SourceSystemId, WorkItemId,
};

#[cfg(test)]
mod command_identity_tests {
    use super::LegacyImportCommandIdentity;

    #[test]
    fn command_identity_is_stable_and_hides_raw_key() {
        let parts = ["batch-1", "4", "START_APPLY"];
        let identity = LegacyImportCommandIdentity::new(
            "import-command-",
            "actor-1",
            "START_APPLY",
            "batch-1",
            "raw-secret-key",
            &parts,
        );
        let same = LegacyImportCommandIdentity::new(
            "import-command-",
            "actor-1",
            "START_APPLY",
            "batch-1",
            "raw-secret-key",
            &parts,
        );

        assert_eq!(identity, same);
        assert!(!identity.audit_id().contains("raw-secret-key"));
        assert_eq!(identity.fingerprint().len(), 64);
    }

    #[test]
    fn command_identity_distinguishes_field_boundaries() {
        let first = LegacyImportCommandIdentity::new(
            "import-command-",
            "actor-1",
            "START_APPLY",
            "batch-1",
            "key-1",
            &["ab", "c"],
        );
        let second = LegacyImportCommandIdentity::new(
            "import-command-",
            "actor-1",
            "START_APPLY",
            "batch-1",
            "key-1",
            &["a", "bc"],
        );

        assert_ne!(first.fingerprint(), second.fingerprint());
    }
}
