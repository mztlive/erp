//! `mall_card_instance` 与 `mall_card_instance_correction`：卡实例稳定基线及纠错事实
//! （数据模型 §6.17）。
//!
//! 卡实例基线首次成功写入后不可覆盖（§6.17），本文件只提供 `new()`，不提供更新。
//! 纠错链不变式（§6.17）：`correction_no = 1` 才允许无前驱；后续纠错必须锁定链尾、
//! 递增一号并引用链尾，禁止多根或分叉。链尾锁定与「同卡实例唯一」属于跨行/跨实体
//! 约束，由 P2 仓储唯一索引与 P3 事务校验落实（P3 条目：§6.17 纠错链）。
//!
//! D28 敏感字段禁令（P1 §2.1 + 数据模型 §4.5.6）：本文件实体不得出现卡号、卡密、
//! 卡实例绑定手机号或可逆映射；卡号类身份只使用文档定义的
//! `opaque_instance_ref`（不可反推卡号、卡密的稳定引用）与
//! `origin_sales_order_source_identity_id`（`external_identity_map` 稳定身份），
//! 并由内联测试对字段清单做静态断言。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::Result;
use crate::ids::{
    ExternalIdentityMapId, MallCardInstanceCorrectionId, MallCardInstanceId, SalesOrderId,
    SalesOrderRevisionId, WorkItemId,
};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 目标商城代码最大长度。
const MALL_ID_MAX_LEN: usize = 64;
/// 不透明卡实例引用最大长度。
const OPAQUE_REF_MAX_LEN: usize = 256;
/// 来源基线版本最大长度。
const BASELINE_VERSION_MAX_LEN: usize = 64;
/// 纠错值（原值/新值）最大长度。
const CORRECTION_VALUE_MAX_LEN: usize = 256;
/// 纠错依据最大长度。
const REASON_MAX_LEN: usize = 512;
/// 确认人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 卡实例来源类型（数据模型 §6.17：实时或历史基线）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardSourceType {
    /// 实时同步的卡实例基线。
    Realtime,
    /// 历史回填形成的卡实例基线。
    HistoricalBaseline,
}

impl CardSourceType {
    /// 返回来源类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Realtime => "实时",
            Self::HistoricalBaseline => "历史基线",
        }
    }

    /// 返回来源类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Realtime => "realtime",
            Self::HistoricalBaseline => "historical_baseline",
        }
    }
}

/// 卡实例纠错类型（数据模型 §6.17：销售单归属或初始余额纠错）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionType {
    /// 销售单归属纠错。
    SalesOrderAttribution,
    /// 初始余额纠错。
    InitialBalance,
}

impl CorrectionType {
    /// 返回纠错类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::SalesOrderAttribution => "销售单归属纠错",
            Self::InitialBalance => "初始余额纠错",
        }
    }

    /// 返回纠错类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SalesOrderAttribution => "sales_order_attribution",
            Self::InitialBalance => "initial_balance",
        }
    }
}

/// 卡实例基线创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallCardInstanceData {
    /// 来源商城（`source_system.code`）。
    pub mall_id: String,
    /// 不可反推卡号、卡密的稳定引用。
    pub opaque_instance_ref: String,
    /// 原商城卡券销售单的 `external_identity_map` 稳定身份。
    pub origin_sales_order_source_identity_id: ExternalIdentityMapId,
    /// 映射后的 ERP 销售单及基线时生效版本。
    pub origin_sales_order_id: SalesOrderId,
    /// 基线形成时生效的销售单版本。
    pub origin_sales_order_revision_id: SalesOrderRevisionId,
    /// 商城提供时保存的卡实例基线版本，可空。
    pub source_baseline_version: Option<String>,
    /// 初始余额。
    pub initial_balance: Amount,
    /// 基线形成时间。
    pub baseline_at: Instant,
    /// 实时或历史基线。
    pub source_type: CardSourceType,
}

/// 卡实例基线实体（数据模型 §6.17）。
///
/// 稳定基线不可覆盖：首次成功写入后，同一 `(mall_id, opaque_instance_ref)` 的重复
/// 基线只做接收确认或差异，不更新本记录；本实体不提供更新方法。实体只保存
/// `opaque_instance_ref` 等脱敏引用，不含卡号、卡密、绑定手机号（§4.5.6）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallCardInstance {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源商城。
    pub mall_id: String,
    /// 不可反推卡号、卡密的稳定引用。
    pub opaque_instance_ref: String,
    /// 原商城卡券销售单的 `external_identity_map` 稳定身份。
    pub origin_sales_order_source_identity_id: ExternalIdentityMapId,
    /// 映射后的 ERP 销售单。
    pub origin_sales_order_id: SalesOrderId,
    /// 基线形成时生效的销售单版本。
    pub origin_sales_order_revision_id: SalesOrderRevisionId,
    /// 商城提供的卡实例基线版本，可空。
    pub source_baseline_version: Option<String>,
    /// 初始余额。
    pub initial_balance: Amount,
    /// 基线形成时间。
    pub baseline_at: Instant,
    /// 实时或历史基线。
    pub source_type: CardSourceType,
}

impl MallCardInstance {
    /// 创建卡实例基线。
    ///
    /// 完成文本字段的完整校验与规范化；`initial_balance` 必须非负。
    /// 基线首次成功写入后不可覆盖（§6.17），本实体不提供更新。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallCardInstanceId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的卡实例基线实体。
    ///
    /// # 错误
    /// 当必填文本为空/超长，或初始余额为负时返回错误。
    pub fn new(id: MallCardInstanceId, data: MallCardInstanceData) -> Result<Self> {
        let mall_id = normalize_required_text(
            data.mall_id,
            "来源商城不能为空",
            MALL_ID_MAX_LEN,
            "来源商城代码过长",
        )?;
        let opaque_instance_ref = normalize_required_text(
            data.opaque_instance_ref,
            "卡实例稳定引用不能为空",
            OPAQUE_REF_MAX_LEN,
            "卡实例稳定引用过长",
        )?;
        let source_baseline_version = normalize_optional_text(
            data.source_baseline_version,
            "卡实例基线版本",
            BASELINE_VERSION_MAX_LEN,
        )?;
        ensure_non_negative_balance(data.initial_balance)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_id,
            opaque_instance_ref,
            origin_sales_order_source_identity_id: data.origin_sales_order_source_identity_id,
            origin_sales_order_id: data.origin_sales_order_id,
            origin_sales_order_revision_id: data.origin_sales_order_revision_id,
            source_baseline_version,
            initial_balance: data.initial_balance,
            baseline_at: data.baseline_at,
            source_type: data.source_type,
        })
    }
}

/// 卡实例纠错创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallCardInstanceCorrectionData {
    /// 原不可变基线。
    pub mall_card_instance_id: MallCardInstanceId,
    /// 同卡实例递增纠错号（从 1 起）。
    pub correction_no: u32,
    /// 销售单归属或初始余额纠错。
    pub correction_type: CorrectionType,
    /// 原值和经确认的新值。
    pub before_value: String,
    /// 经确认的新值。
    pub after_value: String,
    /// 财务纠错任务。
    pub work_item_id: WorkItemId,
    /// 本次纠错承接的同卡实例上一纠错，可空。
    pub supersedes_correction_id: Option<MallCardInstanceCorrectionId>,
    /// 纠错依据。
    pub reason: String,
    /// 审批人。
    pub approved_by: String,
    /// 审批时间。
    pub approved_at: Instant,
}

/// 卡实例纠错实体（数据模型 §6.17）。
///
/// 纠错是不可变追加事实（§4.5），只提供 `new()`。当前归属或余额纠错值由整条链中
/// 该类型最后一条记录派生，不覆盖基线与旧纠错。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallCardInstanceCorrection {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 原不可变基线。
    pub mall_card_instance_id: MallCardInstanceId,
    /// 同卡实例递增纠错号。
    pub correction_no: u32,
    /// 纠错类型。
    pub correction_type: CorrectionType,
    /// 原值。
    pub before_value: String,
    /// 经确认的新值。
    pub after_value: String,
    /// 财务纠错任务。
    pub work_item_id: WorkItemId,
    /// 本次纠错承接的同卡实例上一纠错，可空。
    pub supersedes_correction_id: Option<MallCardInstanceCorrectionId>,
    /// 纠错依据。
    pub reason: String,
    /// 审批人。
    pub approved_by: String,
    /// 审批时间。
    pub approved_at: Instant,
}

impl MallCardInstanceCorrection {
    /// 创建卡实例纠错。
    ///
    /// 完成文本字段校验与规范化，并强制两条不变式（§6.17）：
    /// - `correction_no` 必须 ≥ 1；
    /// - `correction_no = 1` 时 `supersedes_correction_id` 必须为空，
    ///   大于 1 时必须有前驱（链尾锁定与同实例唯一由 P2/P3 落实）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallCardInstanceCorrectionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的纠错实体。
    ///
    /// # 错误
    /// 当纠错号非法、前后驱不一致或必填文本为空/超长时返回错误。
    pub fn new(id: MallCardInstanceCorrectionId, data: MallCardInstanceCorrectionData) -> Result<Self> {
        if data.correction_no == 0 {
            return Err(crate::errors::Error::from("纠错号必须从 1 开始"));
        }
        let has_predecessor = data.supersedes_correction_id.is_some();
        if (data.correction_no == 1) == has_predecessor {
            return Err(crate::errors::Error::from(
                "纠错号为 1 时不得引用前驱，大于 1 时必须引用链尾前驱",
            ));
        }
        let before_value = normalize_required_text(
            data.before_value,
            "纠错原值不能为空",
            CORRECTION_VALUE_MAX_LEN,
            "纠错原值过长",
        )?;
        let after_value = normalize_required_text(
            data.after_value,
            "纠错新值不能为空",
            CORRECTION_VALUE_MAX_LEN,
            "纠错新值过长",
        )?;
        let reason =
            normalize_required_text(data.reason, "纠错依据不能为空", REASON_MAX_LEN, "纠错依据过长")?;
        let approved_by = normalize_required_text(
            data.approved_by,
            "审批人不能为空",
            ACTOR_MAX_LEN,
            "审批人标识过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_card_instance_id: data.mall_card_instance_id,
            correction_no: data.correction_no,
            correction_type: data.correction_type,
            before_value,
            after_value,
            work_item_id: data.work_item_id,
            supersedes_correction_id: data.supersedes_correction_id,
            reason,
            approved_by,
            approved_at: data.approved_at,
        })
    }
}

/// 校验初始余额非负。
///
/// # 参数
/// * `balance` - 初始余额
///
/// # 返回
/// 余额非负返回 `Ok(())`。
///
/// # 错误
/// 余额为负时返回错误。
fn ensure_non_negative_balance(balance: Amount) -> Result<()> {
    if balance.to_decimal().is_sign_negative() {
        return Err(crate::errors::Error::from("初始余额不能为负"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CardSourceType, CorrectionType, MallCardInstance, MallCardInstanceCorrection,
        MallCardInstanceCorrectionData, MallCardInstanceData,
    };
    use crate::common::time::Instant;
    use crate::ids::{
        ExternalIdentityMapId, MallCardInstanceCorrectionId, MallCardInstanceId, SalesOrderId,
        SalesOrderRevisionId, WorkItemId,
    };
    use crate::money::Amount;
    use std::str::FromStr;

    fn instance_data() -> MallCardInstanceData {
        MallCardInstanceData {
            mall_id: " mall-a ".to_string(),
            opaque_instance_ref: " ref-001 ".to_string(),
            origin_sales_order_source_identity_id: ExternalIdentityMapId::new("eim-1"),
            origin_sales_order_id: SalesOrderId::new("so-1"),
            origin_sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
            source_baseline_version: Some(" v3 ".to_string()),
            initial_balance: Amount::from_str("100.00").unwrap(),
            baseline_at: Instant::from_unix_secs(1_700_000_000),
            source_type: CardSourceType::Realtime,
        }
    }

    fn correction_data(correction_no: u32) -> MallCardInstanceCorrectionData {
        MallCardInstanceCorrectionData {
            mall_card_instance_id: MallCardInstanceId::new("card-1"),
            correction_no,
            correction_type: CorrectionType::InitialBalance,
            before_value: " 100.00 ".to_string(),
            after_value: " 98.50 ".to_string(),
            work_item_id: WorkItemId::new("wi-1"),
            supersedes_correction_id: if correction_no == 1 {
                None
            } else {
                Some(MallCardInstanceCorrectionId::new("corr-1"))
            },
            reason: " 余额核对差异 ".to_string(),
            approved_by: " fin-1 ".to_string(),
            approved_at: Instant::from_unix_secs(1_700_000_100),
        }
    }

    /// happy path：文本规范化、余额与归属字段按字典落库。
    #[test]
    fn instance_new_trims_fields_and_keeps_attribution() {
        let instance = MallCardInstance::new(MallCardInstanceId::new("card-1"), instance_data()).unwrap();

        assert_eq!(instance.mall_id, "mall-a");
        assert_eq!(instance.opaque_instance_ref, "ref-001");
        assert_eq!(instance.source_baseline_version.as_deref(), Some("v3"));
        assert_eq!(instance.origin_sales_order_id, SalesOrderId::new("so-1"));
        assert_eq!(
            instance.origin_sales_order_source_identity_id,
            ExternalIdentityMapId::new("eim-1")
        );
        assert_eq!(instance.initial_balance, Amount::from_str("100.00").unwrap());
        assert_eq!(instance.source_type, CardSourceType::Realtime);
    }

    /// 失败路径：必填空、超长、负余额。
    #[test]
    fn instance_new_rejects_empty_overlong_and_negative_balance() {
        let empty = MallCardInstanceData {
            opaque_instance_ref: "   ".to_string(),
            ..instance_data()
        };
        assert!(MallCardInstance::new(MallCardInstanceId::new("card-2"), empty).is_err());

        let overlong = MallCardInstanceData {
            opaque_instance_ref: "r".repeat(257),
            ..instance_data()
        };
        assert!(MallCardInstance::new(MallCardInstanceId::new("card-3"), overlong).is_err());

        let negative = MallCardInstanceData {
            initial_balance: Amount::from_str("-0.01").unwrap(),
            ..instance_data()
        };
        assert!(MallCardInstance::new(MallCardInstanceId::new("card-4"), negative).is_err());
    }

    /// 失败路径：纠错号 0、纠错号与前驱不一致。
    #[test]
    fn correction_new_rejects_invalid_no_and_predecessor_mismatch() {
        let zero = MallCardInstanceCorrectionData {
            correction_no: 0,
            supersedes_correction_id: None,
            ..correction_data(1)
        };
        assert!(MallCardInstanceCorrection::new(MallCardInstanceCorrectionId::new("corr-9"), zero).is_err());

        let no1_with_predecessor = MallCardInstanceCorrectionData {
            supersedes_correction_id: Some(MallCardInstanceCorrectionId::new("corr-0")),
            ..correction_data(1)
        };
        assert!(MallCardInstanceCorrection::new(
            MallCardInstanceCorrectionId::new("corr-10"),
            no1_with_predecessor,
        )
        .is_err());

        let no2_without_predecessor = MallCardInstanceCorrectionData {
            supersedes_correction_id: None,
            ..correction_data(2)
        };
        assert!(MallCardInstanceCorrection::new(
            MallCardInstanceCorrectionId::new("corr-11"),
            no2_without_predecessor,
        )
        .is_err());
    }

    /// happy path + 失败路径：纠错文本规范化、超长与空原因被拒。
    #[test]
    fn correction_new_trims_and_rejects_overlong_text() {
        let correction =
            MallCardInstanceCorrection::new(MallCardInstanceCorrectionId::new("corr-1"), correction_data(1))
                .unwrap();
        assert_eq!(correction.before_value, "100.00");
        assert_eq!(correction.after_value, "98.50");
        assert_eq!(correction.reason, "余额核对差异");
        assert_eq!(correction.correction_no, 1);
        assert!(correction.supersedes_correction_id.is_none());

        let overlong = MallCardInstanceCorrectionData {
            reason: "x".repeat(513),
            ..correction_data(1)
        };
        assert!(
            MallCardInstanceCorrection::new(MallCardInstanceCorrectionId::new("corr-12"), overlong,).is_err()
        );
    }

    /// 敏感字段（P1 §2.1 + §4.5.6）：实体字段清单不含卡号、卡密、手机号及其可逆映射。
    #[test]
    fn entities_do_not_hold_forbidden_card_fields() {
        let instance = MallCardInstance::new(MallCardInstanceId::new("card-1"), instance_data()).unwrap();
        let keys = forbidden_field_assertion(&instance);
        assert!(keys, "mall_card_instance 不得出现卡号/卡密/手机号字段");

        let correction =
            MallCardInstanceCorrection::new(MallCardInstanceCorrectionId::new("corr-1"), correction_data(1))
                .unwrap();
        let keys = forbidden_field_assertion(&correction);
        assert!(keys, "mall_card_instance_correction 不得出现卡号/卡密/手机号字段");
    }

    /// 静态断言辅助：序列化后校验字段名集合，确认不含禁项。
    ///
    /// # 参数
    /// * `entity` - 任意可序列化实体
    ///
    /// # 返回
    /// 字段名集合不含任何禁项时返回 `true`。
    fn forbidden_field_assertion(entity: &impl serde::Serialize) -> bool {
        let value = serde_json::to_value(entity).unwrap();
        let keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        let forbidden = [
            "card_no",
            "card_number",
            "card_secret",
            "card_password",
            "phone",
            "mobile",
            "bound_phone",
        ];
        forbidden.iter().all(|key| !keys.contains(key))
    }
}
