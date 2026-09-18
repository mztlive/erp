//! 收据分支、当前 V3 身份与有限历史候选。

use std::time::Duration;

use bpm::model::types::ApprovalCommandKind;
use bpm::model::{ApprovalCommandIdentity, ApprovalCommandReceipt, CanonicalCommandPayload, IdempotencyKey};

use crate::error::{Error, ErrorCode, Result};

/// 收据查找后的分支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptBranch<'a> {
    /// 收据不存在，允许执行命令。
    Fresh,
    /// 同载荷，只允许授权回读。
    SamePayload(&'a ApprovalCommandReceipt),
    /// 异载荷或身份降级冲突。
    PayloadConflict,
}

/// 一个历史收据只允许以原 scope 与原 digest 成对匹配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyReceiptIdentity {
    scope: String,
    digest: String,
}

impl LegacyReceiptIdentity {
    /// 创建一个已知历史 writer 的精确身份候选。
    pub fn exact(scope: impl Into<String>, digest: impl Into<String>) -> Self {
        Self { scope: scope.into(), digest: digest.into() }
    }
}

/// 一个审批运行命令的当前 V3 身份及其有限历史读取候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCommandIdentity {
    current: ApprovalCommandIdentity,
    legacy: Vec<LegacyReceiptIdentity>,
}

impl PreparedCommandIdentity {
    /// 用当前 V3 身份与精确历史候选构造命令身份。
    ///
    /// # 参数
    /// * `current` - 当前 writer 的 V3 身份
    /// * `legacy` - 已冻结的精确历史候选
    ///
    /// # 返回
    /// 返回可分类收据的命令身份。
    ///
    /// # 错误
    /// 无。
    pub(super) fn new(current: ApprovalCommandIdentity, legacy: Vec<LegacyReceiptIdentity>) -> Self {
        Self { current, legacy }
    }

    /// 返回当前 writer 唯一允许写入的 V3 身份。
    pub fn current(&self) -> &ApprovalCommandIdentity {
        &self.current
    }

    /// 返回已规范化幂等键。
    pub fn idempotency_key(&self) -> &IdempotencyKey {
        self.current.idempotency_key()
    }

    /// 返回 receipt 查询 scope，严格按 V3、已知历史格式顺序去重。
    ///
    /// 复杂度上限：候选数 = 1（V3）+ 已登记历史数（构造时去重冻结，通常 ≤ 3）；
    /// 查询时线性扫描上界固定，不随单据量增长。
    pub fn scope_candidates(&self) -> Vec<&str> {
        let mut scopes = vec![self.current.scope().as_str()];
        for candidate in &self.legacy {
            if !scopes.contains(&candidate.scope.as_str()) {
                scopes.push(candidate.scope.as_str());
            }
        }
        scopes
    }

    /// 追加一个调用方拥有的已知历史 writer 身份。
    ///
    /// 仅用于兼容已经持久化的显式版本格式；不得把模糊组合、任意旧摘要或
    /// V3 scope/digest 的交叉组合登记为候选。
    pub fn with_legacy(mut self, candidate: LegacyReceiptIdentity) -> Self {
        if !self.legacy.contains(&candidate) {
            self.legacy.push(candidate);
        }
        self
    }

    /// 按完整命令身份分类收据。
    ///
    /// 当前 V3 scope 只接受当前 V3 digest。历史收据只接受登记时成对保存的
    /// scope 与 digest，禁止 scope/digest 交叉组合或 V3 降级匹配。
    pub fn classify<'a>(&self, receipt: Option<&'a ApprovalCommandReceipt>) -> ReceiptBranch<'a> {
        let Some(receipt) = receipt else {
            return ReceiptBranch::Fresh;
        };
        if receipt.command_kind != self.current.command_kind()
            || &receipt.idempotency_key != self.current.idempotency_key()
        {
            return ReceiptBranch::PayloadConflict;
        }
        if receipt.scope_id == self.current.scope().as_str() {
            return if receipt.payload_digest == self.current.digest().as_str() {
                ReceiptBranch::SamePayload(receipt)
            } else {
                ReceiptBranch::PayloadConflict
            };
        }
        if is_v3_hash(&receipt.scope_id) || is_v3_hash(&receipt.payload_digest) {
            return ReceiptBranch::PayloadConflict;
        }
        if self.legacy.iter().any(|candidate| {
            candidate.scope == receipt.scope_id && candidate.digest == receipt.payload_digest
        }) {
            ReceiptBranch::SamePayload(receipt)
        } else {
            ReceiptBranch::PayloadConflict
        }
    }
}

fn is_v3_hash(value: &str) -> bool {
    value
        .strip_prefix("v3:")
        .is_some_and(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// 在任何仓储查询前形成规范化幂等键。
///
/// # 错误
/// 空值、超长值或其他 BPM 值对象约束不满足时返回校验错误。
pub fn normalize_idempotency_key(raw: &str) -> Result<IdempotencyKey> {
    IdempotencyKey::parse(raw).map_err(|error| Error::ValidationError(error.to_string()))
}

/// 构造当前 writer 唯一允许写入的 V3 命令身份。
///
/// # 参数
/// * `kind` - 命令种类
/// * `domain` - 身份域
/// * `key` - 已规范化幂等键
/// * `scope_payload` - 当前 scope 载荷
/// * `digest_payload` - 当前 digest 载荷
///
/// # 返回
/// 返回 V3 `ApprovalCommandIdentity`。
///
/// # 错误
/// 幂等键或载荷字段非法时返回校验错误。
pub(super) fn current_identity(
    kind: ApprovalCommandKind,
    domain: &str,
    key: IdempotencyKey,
    scope_payload: CanonicalCommandPayload,
    digest_payload: CanonicalCommandPayload,
) -> Result<ApprovalCommandIdentity> {
    ApprovalCommandIdentity::new(kind, domain, key, scope_payload, digest_payload)
        .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 幂等冲突的稳定错误。
pub fn payload_conflict_error() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalIdempotencyPayloadConflict)
}

/// 映射 receipt-first 第一笔写错误。
///
/// 仅审批命令收据 identity 唯一索引竞争允许退出失败会话后回读；收据主键、
/// 未知索引或其他业务集合唯一冲突均失败关闭。
pub fn map_receipt_first_write_error(error: persistence_core::Error) -> Error {
    if error.duplicate_index_name()
        == Some(crate::repository::bpm::APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX)
    {
        Error::ReceiptDuplicate(error)
    } else {
        Error::from(error)
    }
}

/// 判断命令是否只允许在新会话有限回读原结果。
pub fn command_may_have_committed(error: &Error) -> bool {
    matches!(error, Error::OutcomeUnknown(_) | Error::ReceiptDuplicate(_) | Error::TransientTransaction(_))
}

/// 返回命令结果有限回读的指数退避，上限为 160ms。
pub fn command_recovery_delay(attempt: usize) -> Duration {
    let shift = u32::try_from(attempt).unwrap_or(u32::MAX).min(5);
    Duration::from_millis(5_u64 << shift)
}
