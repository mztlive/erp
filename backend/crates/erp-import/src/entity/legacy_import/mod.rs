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
pub mod import_row_factory;
pub mod legacy_import_batch;
pub mod legacy_import_confirmation;
pub mod legacy_import_row;

pub use apply_result_set::{ApplyResultDraft, ApplyResultItem, ApplyResultOutcome, ApplyResultSet};
pub use import_row_factory::{ImportRowSpec, build_import_rows};
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
        Self { audit_id, fingerprint: encode_digest(digest.finalize()) }
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

/// 按首次出现顺序保序去重，返回去重后的元素引用（三处行 ID 去重共用原语）。
///
/// 键归一化口径由调用方 `key` 决定（是否 trim）：`apply_scope` 按行 ID 原值，
/// 行工厂按去首尾空白后的 `(object_type, row_key)` 元组，`apply_result_set`
/// 按行 ID 原值。语义差异保留在调用方，本函数只收敛保序去重样板。
///
/// # 参数
/// * `items` - 待去重元素切片
/// * `key` - 去重键提取（含调用方约定的归一化口径）
///
/// # 返回
/// 返回首次出现顺序的元素引用。
pub fn dedupe_by_key<T, K>(items: &[T], mut key: impl FnMut(&T) -> K) -> Vec<&T>
where
    K: Eq + std::hash::Hash,
{
    let mut seen = std::collections::HashSet::new();
    items.iter().filter(|item| seen.insert(key(item))).collect()
}

/// 查找首个重复键（存在重复时返回该键；无重复返回 `None`）。
///
/// 键口径约定同 [`dedupe_by_key`]；调用方保留各自的重复错误文案。
///
/// # 参数
/// * `items` - 待检查元素切片
/// * `key` - 去重键提取（含调用方约定的归一化口径）
///
/// # 返回
/// 返回首个重复键；无重复时返回 `None`。
pub fn first_duplicate_key<'a, T: 'a, K>(items: &'a [T], mut key: impl FnMut(&'a T) -> K) -> Option<K>
where
    K: Eq + std::hash::Hash,
{
    let mut seen = std::collections::HashSet::new();
    items.iter().find(|item| !seen.insert(key(item))).map(|item| key(item))
}

/// 将摘要字节编码为小写十六进制文本（与 `format!("{digest:x}")` 同形态）。
///
/// # 参数
/// * `digest` - SHA-256 摘要字节
///
/// # 返回
/// 返回 64 位小写十六进制摘要。
fn encode_digest(digest: impl AsRef<[u8]>) -> String {
    digest.as_ref().iter().map(|byte| format!("{byte:02x}")).collect()
}

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use erp_core::ids::{
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
