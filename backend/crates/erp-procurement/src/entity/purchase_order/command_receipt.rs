//! 采购领域独立命令回执：稳定身份、版本化指纹与强类型首次结果。

use std::str::FromStr;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::money::{Amount, Quantity};
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{CreationReceipt, PurchaseSubmitReceipt, SaveDraftReceipt, SourcingReceipt, VoidDraftReceipt};
use crate::dto::purchase_order::{
    CREATE_ACTION, CREATE_SOURCING_ACTION, PURCHASE_SUBMIT_ACTION, SAVE_ACTION, VOID_ACTION,
};

/// 原采购长度前缀 SHA-256 算法的版本化摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseCommandFingerprint {
    /// 指纹 schema 版本。
    pub schema_version: u16,
    /// 固定算法代码。
    pub algorithm: String,
    /// 64 位小写十六进制摘要。
    pub digest: String,
}
impl PurchaseCommandFingerprint {
    /// 为原算法摘要附加显式版本；非法摘要拒绝持久化。
    fn new(digest: &str) -> Result<Self> {
        let value =
            Self { schema_version: 1, algorithm: "sha256-length-prefixed-v1".into(), digest: digest.into() };
        if !value.valid() {
            return Err(Error::from("采购命令指纹格式无效"));
        }
        Ok(value)
    }
    /// 验证持久化版本、算法与规范摘要。
    fn valid(&self) -> bool {
        self.schema_version == 1
            && self.algorithm == "sha256-length-prefixed-v1"
            && self.digest.len() == 64
            && self.digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
}

/// 采购命令完整身份；不保存原幂等键或旧审计 ID 候选。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseCommandReceiptIdentity {
    /// 身份 schema 版本。
    pub schema_version: u16,
    /// 原长度前缀算法产生的稳定命令 ID。
    pub command_id: String,
    /// 已认证操作人。
    pub actor_id: String,
    /// 固定服务端动作。
    pub action: String,
    /// 结果资源类别。
    pub resource_type: String,
    /// 命令目标；创建命令为空。
    pub scope_id: Option<String>,
    /// 原幂等键不可逆摘要。
    pub idempotency_key_digest: PurchaseCommandFingerprint,
}
impl PurchaseCommandReceiptIdentity {
    /// 返回本命令唯一持久化 ID。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回原稳定命令 ID。
    /// # 错误
    /// 无。
    pub fn receipt_id(&self) -> &str {
        &self.command_id
    }
    /// 验证身份形态；未知 schema 或空身份阻断恢复。
    fn valid(&self) -> bool {
        self.schema_version == 1
            && self.resource_type == "purchase_order"
            && [&self.command_id, &self.actor_id, &self.action].into_iter().all(|v| !v.trim().is_empty())
            && self.scope_id.as_ref().is_none_or(|v| !v.trim().is_empty())
            && self.idempotency_key_digest.valid()
    }
}

/// 回放分类；调用方保持原命令错误映射。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurchaseCommandReceiptError {
    /// 命令身份不匹配。
    IdentityMismatch,
    /// 同键已提交不同载荷。
    PayloadConflict,
    /// 持久化 schema、身份或结果损坏。
    Corrupted(String),
}

/// 采购领域拥有的强类型结果合同。
pub trait PurchaseReceiptResult: sealed::Sealed + Serialize + DeserializeOwned + Clone + Send + Sync {
    /// 验证动作目录与结果资源对应关系。
    ///
    /// # 参数
    /// * `identity` - 当前命令身份。
    /// # 返回
    /// 返回结果是否符合持久化合同。
    /// # 错误
    /// 无；非法结果返回 `false`。
    fn valid_result(&self, identity: &PurchaseCommandReceiptIdentity) -> bool;
}

/// 只有采购拥有的结果类型可绑定采购回执仓储。
mod sealed {
    pub trait Sealed {}
    impl Sealed for super::CreationReceipt {}
    impl Sealed for super::SaveDraftReceipt {}
    impl Sealed for super::VoidDraftReceipt {}
    impl Sealed for super::SourcingReceipt {}
    impl Sealed for super::PurchaseSubmitReceipt {}
}

/// 与正式事实及审计同事务提交的不可变采购回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct PurchaseCommandReceipt<T> {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 身份、定位与幂等键摘要。
    pub command: PurchaseCommandReceiptIdentity,
    /// 原规范化请求摘要。
    pub fingerprint: PurchaseCommandFingerprint,
    /// 强类型结果 schema 版本。
    pub result_schema_version: u16,
    /// 首次成功结果。
    pub result: T,
    /// 关联审计事件；恢复不查询审计。
    pub audit_event_id: String,
}
impl<T> PurchaseCommandReceipt<T> {
    /// 依据原算法生成稳定身份，原幂等键只参与摘要。
    ///
    /// # 参数
    /// * `prefix` - 命令 ID 前缀。
    /// * `actor_id` - 已认证操作人。
    /// * `action` - 固定命令动作。
    /// * `target_id` - 命令目标；创建命令为空。
    /// * `idempotency_key` - 原幂等键。
    /// # 返回
    /// 返回本命令唯一完整身份。
    /// # 错误
    /// 必填身份为空时返回错误。
    pub fn identity(
        prefix: &str,
        actor_id: &str,
        action: &str,
        target_id: Option<&str>,
        idempotency_key: &str,
    ) -> Result<PurchaseCommandReceiptIdentity> {
        if [prefix, actor_id, action, idempotency_key].into_iter().any(|v| v.trim().is_empty())
            || target_id.is_some_and(|v| v.trim().is_empty())
        {
            return Err(Error::from("采购命令身份不能为空"));
        }
        let mut parts = vec![actor_id, action];
        if let Some(target) = target_id {
            parts.push(target);
        }
        parts.push(idempotency_key);
        Ok(PurchaseCommandReceiptIdentity {
            schema_version: 1,
            command_id: format!("{prefix}{}", digest_parts(parts)),
            actor_id: actor_id.into(),
            action: action.into(),
            resource_type: "purchase_order".into(),
            scope_id: target_id.map(str::to_string),
            idempotency_key_digest: PurchaseCommandFingerprint::new(&digest_parts([idempotency_key]))?,
        })
    }
    /// 读取首次结果引用。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回首次提交结果引用。
    /// # 错误
    /// 无。
    pub fn payload(&self) -> &T {
        &self.result
    }
    /// 取得首次成功结果。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回首次提交结果。
    /// # 错误
    /// 无。
    pub fn into_payload(self) -> T {
        self.result
    }
}
impl<T: PurchaseReceiptResult> PurchaseCommandReceipt<T> {
    /// 构造与正式事实同事务提交的独立回执。
    ///
    /// # 参数
    /// * `identity` - 当前完整命令身份。
    /// * `fingerprint` - 原规范化请求摘要。
    /// * `result` - 首次成功强类型结果。
    /// * `audit_event_id` - 同事务关联审计事件。
    /// # 返回
    /// 返回合法回执。
    /// # 错误
    /// 身份、指纹或结果无效时返回错误。
    pub fn new(
        identity: &PurchaseCommandReceiptIdentity,
        fingerprint: &str,
        result: T,
        audit_event_id: String,
    ) -> Result<Self> {
        let value = Self {
            base: BaseModel::new(identity.command_id.clone()),
            command: identity.clone(),
            fingerprint: PurchaseCommandFingerprint::new(fingerprint)?,
            result_schema_version: 1,
            result,
            audit_event_id,
        };
        if !value.valid() {
            return Err(Error::from("采购命令回执结果无效"));
        }
        Ok(value)
    }
    /// 校验真实仓储反序列化后的独立回执。
    ///
    /// # 参数
    /// * `record` - 独立领域回执。
    /// * `identity` - 本次请求完整身份。
    /// * `fingerprint` - 当前规范化请求摘要。
    /// # 返回
    /// 同身份同载荷返回原结果回执。
    /// # 错误
    /// 身份、载荷不一致或持久化记录损坏时返回对应分类。
    pub fn decode(
        record: Self,
        identity: &PurchaseCommandReceiptIdentity,
        fingerprint: &str,
    ) -> std::result::Result<Self, PurchaseCommandReceiptError> {
        if !record.valid() {
            return Err(PurchaseCommandReceiptError::Corrupted("采购命令回执损坏".into()));
        }
        if &record.command != identity {
            return Err(PurchaseCommandReceiptError::IdentityMismatch);
        }
        if record.fingerprint.digest != fingerprint {
            return Err(PurchaseCommandReceiptError::PayloadConflict);
        }
        Ok(record)
    }
    /// 校验不可变记录；删除或未知 schema 必须阻断恢复。
    fn valid(&self) -> bool {
        self.command.valid()
            && self.fingerprint.valid()
            && self.result_schema_version == 1
            && self.base.id == self.command.command_id
            && !self.base.is_deleted()
            && !self.audit_event_id.trim().is_empty()
            && self.result.valid_result(&self.command)
    }
}
impl PurchaseReceiptResult for CreationReceipt {
    fn valid_result(&self, id: &PurchaseCommandReceiptIdentity) -> bool {
        [CREATE_ACTION, CREATE_SOURCING_ACTION].contains(&id.action.as_str())
            && !self.purchase_order_id.trim().is_empty()
            && !self.purchase_no.trim().is_empty()
            && self.lock_version > 0
    }
}
impl PurchaseReceiptResult for SaveDraftReceipt {
    fn valid_result(&self, id: &PurchaseCommandReceiptIdentity) -> bool {
        self.lock_version > 0
            && valid_saved_totals(self)
            && id.action == SAVE_ACTION
            && id.scope_id.as_deref() == Some(self.purchase_order_id.as_str())
            && [&self.purchase_order_id, &self.gross, &self.net, &self.tax, &self.reference]
                .into_iter()
                .all(|v| !v.trim().is_empty())
    }
}
impl PurchaseReceiptResult for VoidDraftReceipt {
    fn valid_result(&self, id: &PurchaseCommandReceiptIdentity) -> bool {
        self.lock_version > 0
            && !self.reason.trim().is_empty()
            && id.action == VOID_ACTION
            && id.scope_id.as_deref() == Some(self.purchase_order_id.as_str())
            && self.status == "VOIDED"
            && !self.reference.trim().is_empty()
    }
}
impl PurchaseReceiptResult for SourcingReceipt {
    fn valid_result(&self, id: &PurchaseCommandReceiptIdentity) -> bool {
        id.action == CREATE_SOURCING_ACTION
            && id.scope_id.is_some()
            && self.orders.iter().all(|v| {
                !v.purchase_order_id.trim().is_empty()
                    && !v.purchase_no.trim().is_empty()
                    && v.lock_version > 0
            })
            && self.stock_reservations.iter().all(|v| {
                Quantity::from_str(&v.quantity).is_ok_and(|quantity| quantity.to_decimal() > Decimal::ZERO)
                    && [
                        &v.stock_reservation_id,
                        &v.sales_order_line_id,
                        &v.stock_balance_id,
                        &v.warehouse_id,
                        &v.quantity,
                    ]
                    .into_iter()
                    .all(|v| !v.trim().is_empty())
            })
    }
}
impl PurchaseReceiptResult for PurchaseSubmitReceipt {
    fn valid_result(&self, id: &PurchaseCommandReceiptIdentity) -> bool {
        self.lock_version > 0
            && self.subject_version.parse::<u32>().is_ok_and(|v| v > 0)
            && id.action == PURCHASE_SUBMIT_ACTION
            && id.scope_id.is_some()
            && [&self.purchase_no, &self.submission_id, &self.submission_no, &self.subject_version]
                .into_iter()
                .all(|v| !v.trim().is_empty())
            && (self.work_item_id.is_empty() == (self.task_version == 0))
            && (self.work_item_id.is_empty() || !self.work_item_id.trim().is_empty())
    }
}

/// 损坏金额不得以非空字符串冒充首次保存事实。
fn valid_saved_totals(value: &SaveDraftReceipt) -> bool {
    let (Ok(gross), Ok(net), Ok(tax)) =
        (Amount::from_str(&value.gross), Amount::from_str(&value.net), Amount::from_str(&value.tax))
    else {
        return false;
    };
    [gross, net, tax].into_iter().all(|v| v.to_decimal() >= Decimal::ZERO)
        && net.to_decimal().checked_add(tax.to_decimal()) == Some(gross.to_decimal())
}

/// 对原顺序文本片段计算长度前缀 SHA-256，保持原采购算法。
///
/// # 参数
/// * `parts` - 原业务顺序的文本片段。
/// # 返回
/// 返回64位规范摘要。
/// # 错误
/// 无。
pub fn digest_parts<I, S>(parts: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut hasher = Sha256::new();
    for part in parts {
        let part = part.as_ref();
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// 保留原采购请求的固定动作、目标与规范 JSON 摘要。
///
/// # 参数
/// * `action` - 原动作。
/// * `target_id` - 原目标。
/// * `payload` - 已排除幂等键的规范化请求。
/// # 返回
/// 返回原算法摘要。
/// # 错误
/// 序列化失败时返回错误。
pub fn payload_fingerprint<T: Serialize>(action: &str, target_id: &str, payload: &T) -> Result<String> {
    let payload = serde_json::to_string(payload)
        .map_err(|e| Error::from(format!("采购命令请求指纹序列化失败: {e}")))?;
    Ok(digest_parts([action, target_id, payload.as_str()]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::purchase_order::{SourcingOrderReceipt, SourcingTaskStatus};
    /// 真实字段反序列化后调用生产 decoder，验证首结果及冲突分类。
    fn round_trip<T: PurchaseReceiptResult + std::fmt::Debug + PartialEq>(
        action: &str,
        scope: Option<&str>,
        payload: T,
    ) {
        let identity = PurchaseCommandReceipt::<T>::identity(
            "purchase-command-",
            "actor",
            action,
            scope,
            "raw-secret-key",
        )
        .unwrap();
        let record =
            PurchaseCommandReceipt::new(&identity, &"a".repeat(64), payload.clone(), "audit-event".into())
                .unwrap();
        let stored = serde_json::to_string(&record).unwrap();
        assert!(!stored.contains("raw-secret-key"));
        let decoded: PurchaseCommandReceipt<T> = serde_json::from_str(&stored).unwrap();
        assert_eq!(
            PurchaseCommandReceipt::decode(decoded.clone(), &identity, &"a".repeat(64))
                .unwrap()
                .into_payload(),
            payload
        );
        assert!(matches!(
            PurchaseCommandReceipt::decode(decoded.clone(), &identity, &"b".repeat(64)),
            Err(PurchaseCommandReceiptError::PayloadConflict)
        ));
        let mut corrupted = decoded.clone();
        corrupted.result_schema_version = 2;
        assert!(matches!(
            PurchaseCommandReceipt::decode(corrupted, &identity, &"a".repeat(64)),
            Err(PurchaseCommandReceiptError::Corrupted(_))
        ));
        let mut other_identity = identity.clone();
        other_identity.idempotency_key_digest.digest = "b".repeat(64);
        assert!(matches!(
            PurchaseCommandReceipt::decode(decoded, &other_identity, &"a".repeat(64)),
            Err(PurchaseCommandReceiptError::IdentityMismatch)
        ));
    }
    #[test]
    fn five_procurement_results_use_real_typed_decoder() {
        round_trip(
            CREATE_ACTION,
            None,
            CreationReceipt { purchase_order_id: "po-1".into(), purchase_no: "PO-1".into(), lock_version: 2 },
        );
        round_trip(
            SAVE_ACTION,
            Some("po-1"),
            SaveDraftReceipt {
                purchase_order_id: "po-1".into(),
                lock_version: 3,
                gross: "100.00".into(),
                net: "90.00".into(),
                tax: "10.00".into(),
                reference: "SAVED-V3".into(),
            },
        );
        round_trip(
            VOID_ACTION,
            Some("po-1"),
            VoidDraftReceipt {
                purchase_order_id: "po-1".into(),
                status: "VOIDED".into(),
                lock_version: 4,
                reason: "cancelled".into(),
                reference: "VOID-V4".into(),
            },
        );
        round_trip(
            CREATE_SOURCING_ACTION,
            Some("so-1"),
            SourcingReceipt {
                orders: vec![SourcingOrderReceipt {
                    purchase_order_id: "po-1".into(),
                    purchase_no: "PO-1".into(),
                    lock_version: 2,
                }],
                stock_reservations: vec![crate::dto::purchase_order::ExistingStockReservationResult {
                    stock_reservation_id: "reservation-1".into(),
                    sales_order_line_id: "line-1".into(),
                    stock_balance_id: "balance-1".into(),
                    warehouse_id: "warehouse-1".into(),
                    quantity: "2.000000".into(),
                }],
                work_item_status: SourcingTaskStatus::Open,
            },
        );
        round_trip(
            PURCHASE_SUBMIT_ACTION,
            Some("po-1"),
            PurchaseSubmitReceipt::new(
                "PO-1".into(),
                "sub-1".into(),
                "SUB-1".into(),
                String::new(),
                "1".into(),
            )
            .with_versions(0, 2)
            .with_first_task(Some(&("wi-1".into(), 1))),
        );
    }
    #[test]
    fn required_sourcing_status_and_unknown_algorithm_fail_closed() {
        assert!(serde_json::from_str::<SourcingReceipt>(r#"{"orders":[],"stock_reservations":[]}"#).is_err());
        let identity = PurchaseCommandReceipt::<CreationReceipt>::identity(
            "purchase-command-",
            "actor",
            CREATE_ACTION,
            None,
            "key",
        )
        .unwrap();
        let mut record = PurchaseCommandReceipt::new(
            &identity,
            &"a".repeat(64),
            CreationReceipt { purchase_order_id: "po-1".into(), purchase_no: "PO-1".into(), lock_version: 2 },
            "event".into(),
        )
        .unwrap();
        record.fingerprint.algorithm = "unknown".into();
        assert!(matches!(
            PurchaseCommandReceipt::decode(record, &identity, &"a".repeat(64)),
            Err(PurchaseCommandReceiptError::Corrupted(_))
        ));
        assert_ne!(digest_parts(["ab", "c"]), digest_parts(["a", "bc"]));
    }

    /// 原命令 ID 保留固定黄金值，库存来源不会随独立审计事件 ID 改变。
    #[test]
    fn stable_identity_retains_original_length_prefixed_algorithm() {
        let identity = PurchaseCommandReceipt::<PurchaseSubmitReceipt>::identity(
            "purchase-submit-command-",
            "actor-1",
            PURCHASE_SUBMIT_ACTION,
            Some("po-1"),
            "legacy-key",
        )
        .unwrap();
        assert_eq!(
            identity.receipt_id(),
            "purchase-submit-command-dadfeb01ed3abc920581f621e8a09c1fbaa34254b6da72b1662e07f9cd6ed110"
        );
    }

    /// 删除、错资源、非法金额或缺审计关联均不得以无回执分支重执行。
    #[test]
    fn corrupted_identity_or_saved_snapshot_blocks_replay() {
        let identity = PurchaseCommandReceipt::<SaveDraftReceipt>::identity(
            "purchase-save-",
            "actor-1",
            SAVE_ACTION,
            Some("po-1"),
            "key",
        )
        .unwrap();
        let record = PurchaseCommandReceipt::new(
            &identity,
            &"a".repeat(64),
            SaveDraftReceipt {
                purchase_order_id: "po-1".into(),
                lock_version: 3,
                gross: "100.00".into(),
                net: "90.00".into(),
                tax: "10.00".into(),
                reference: "SAVED-V3".into(),
            },
            "event".into(),
        )
        .unwrap();
        let mut cases = Vec::new();
        let mut changed = record.clone();
        changed.base.deleted_at = 1;
        cases.push(changed);
        let mut changed = record.clone();
        changed.base.id = "other-id".into();
        cases.push(changed);
        let mut changed = record.clone();
        changed.command.scope_id = Some("po-2".into());
        cases.push(changed);
        let mut changed = record.clone();
        changed.result.purchase_order_id = "po-2".into();
        cases.push(changed);
        let mut changed = record.clone();
        changed.result.net = "not-a-number".into();
        cases.push(changed);
        let mut changed = record.clone();
        changed.result.tax = "20.00".into();
        cases.push(changed);
        let mut changed = record.clone();
        changed.audit_event_id.clear();
        cases.push(changed);
        let mut changed = record.clone();
        changed.command.idempotency_key_digest.schema_version = 2;
        cases.push(changed);
        for changed in cases {
            let encoded = serde_json::to_string(&changed).unwrap();
            let decoded = serde_json::from_str(&encoded).unwrap();
            assert!(matches!(
                PurchaseCommandReceipt::<SaveDraftReceipt>::decode(decoded, &identity, &"a".repeat(64)),
                Err(PurchaseCommandReceiptError::Corrupted(_))
            ));
        }
        let mut other_actor = record;
        other_actor.command.actor_id = "actor-2".into();
        assert!(matches!(
            PurchaseCommandReceipt::decode(other_actor, &identity, &"a".repeat(64)),
            Err(PurchaseCommandReceiptError::IdentityMismatch)
        ));
    }
}
