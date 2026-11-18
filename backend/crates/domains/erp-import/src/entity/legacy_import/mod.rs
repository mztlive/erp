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
//! 确认矩阵、版本、行状态与命令幂等身份由本域类型确定；跨集合写入由本域仓储
//! 在调用方事务内执行，正式应用由组合层 Process 编排，`work_item` 由工作流域持有。

use sha2::{Digest, Sha256};

pub mod apply_result_set;
mod confirmation_completion;
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
    command_id: String,
    fingerprint: String,
    actor_id: String,
    action: String,
    scope_id: String,
    key_hash: application_core::CommandFingerprint,
}

impl LegacyImportCommandIdentity {
    /// 从操作人、动作、资源及客户端幂等键派生稳定命令 ID，并对完整规范化载荷逐字段加长度前缀计算 SHA-256 指纹；不保存原键，避免拼接歧义并识别同键异参。
    pub fn new(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_id: &str,
        idempotency_key: &str,
        parts: &[&str],
    ) -> Self {
        let command_id = format!(
            "{prefix}{}",
            sha256_hex(format!("{actor_id}|{action}|{resource_id}|{idempotency_key}").as_bytes())
        );

        let mut digest = Sha256::new();
        for part in parts {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part.as_bytes());
        }

        Self {
            command_id,
            fingerprint: encode_digest(digest.finalize()),
            actor_id: actor_id.to_string(),
            action: action.to_string(),
            scope_id: resource_id.to_string(),
            key_hash: application_core::CommandFingerprint::from_parts([idempotency_key.to_string()]),
        }
    }

    /// 提供 SHA-256 派生的稳定命令 ID，用于定位回执而不暴露原幂等键。
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    /// 提供长度前缀编码后计算的 SHA-256 指纹，用于核对同一幂等键的完整载荷。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 将原命令 ID 与完整载荷摘要写入固定资源类型的 v1 结构化回执身份；身份不完整则拒绝，不查旧审计。
    pub fn structured_receipt(
        &self,
        resource_type: &str,
    ) -> crate::Result<application_core::StructuredCommandReceipt> {
        let header = application_core::StructuredCommandReceipt {
            schema_version: 1,
            command_id: self.command_id.clone(),
            actor_id: self.actor_id.clone(),
            action: self.action.clone(),
            resource_type: resource_type.to_string(),
            scope_id: Some(self.scope_id.clone()),
            idempotency_key_hash: self.key_hash.clone(),
            fingerprint: application_core::CommandFingerprint::from_parts([self.fingerprint.clone()]),
        };
        header.validate()?;
        Ok(header)
    }
}

// 对输入计算 SHA-256 并编码成 64 个小写十六进制字符，供派生命令身份和指纹。
fn sha256_hex(value: &[u8]) -> String {
    encode_digest(Sha256::digest(value))
}

/// 按键保留首次出现的元素引用，使去重不改变请求顺序。应用范围和结果集使用原始行 ID；行工厂使用去首尾空白后的 `(object_type, row_key)`，由调用方决定键的口径。
pub fn dedupe_by_key<T, K>(items: &[T], mut key: impl FnMut(&T) -> K) -> Vec<&T>
where
    K: Eq + std::hash::Hash,
{
    let mut seen = std::collections::HashSet::new();
    items.iter().filter(|item| seen.insert(key(item))).collect()
}

/// 用与 [`dedupe_by_key`] 相同的调用方键规则找首个重复键，供各入口生成自己的拒绝文案；无重复返回 `None`。
pub fn first_duplicate_key<'a, T: 'a, K>(items: &'a [T], mut key: impl FnMut(&'a T) -> K) -> Option<K>
where
    K: Eq + std::hash::Hash,
{
    let mut seen = std::collections::HashSet::new();
    items.iter().find(|item| !seen.insert(key(item))).map(key)
}

// 将 SHA-256 摘要字节编码成 64 个小写十六进制字符，输出与 `format!("{digest:x}")` 一致，以保持历史身份兼容。
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
        assert!(!identity.command_id().contains("raw-secret-key"));
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
