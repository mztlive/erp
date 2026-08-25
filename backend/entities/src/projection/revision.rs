//! `sales_order_projection_revision`：不可变执行投影版本（数据模型 §6.16，页面 W23）。
//!
//! 投影版本组合 [`crate::common::revision::RevisionBase`]，与 ERP 销售单版本
//! 一一对应（phase-2 §14.2），内联结构化快照：投影来源、ERP 销售版本、商城客户/
//! 卡券类目标识、表头履约期限、唯一卡券明细执行字段（面额/卡张数/卡形态）与
//! 生效时间。投影不含成交金额、配赠、税率、开票和应收（§6.16，字段集即白名单）。

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SalesOrderProjectionId, SalesOrderProjectionRevisionId, SalesOrderRevisionId};
use crate::money::Amount;
use crate::validation::normalize_required_text;

/// 商城客户标识最大长度。
const EXTERNAL_IDENTITY_MAX_LEN: usize = 256;
/// 投影内容指纹最大长度。
const HASH_MAX_LEN: usize = 128;

/// 投影来源（数据模型 §6.16：存量单切换时的当前 ERP 销售版本或后续 ERP 销售版本；
/// 固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionSource {
    /// 存量单切换时点版本（切换时以当时 ERP 当前销售单版本作为第一份执行投影，
    /// 不产生新的销售单版本，phase-2 §8.5.4）。
    CutoverSnapshot,
    /// 后续 ERP 销售版本。
    ErpRevision,
}

impl ProjectionSource {
    /// 返回来源的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::CutoverSnapshot => "存量单切换快照",
            Self::ErpRevision => "ERP 销售版本",
        }
    }

    /// 返回来源的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CutoverSnapshot => "cutover_snapshot",
            Self::ErpRevision => "erp_revision",
        }
    }
}

/// 卡形态（数据模型 §6.16：电子卡或实体卡；固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardForm {
    /// 电子卡。
    Electronic,
    /// 实体卡。
    Physical,
}

impl CardForm {
    /// 返回卡形态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Electronic => "电子卡",
            Self::Physical => "实体卡",
        }
    }

    /// 返回卡形态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Electronic => "electronic",
            Self::Physical => "physical",
        }
    }
}

/// 执行投影版本创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionRevisionData {
    /// 投影稳定身份。
    pub projection_id: SalesOrderProjectionId,
    /// 投影来源。
    pub projection_source: ProjectionSource,
    /// ERP 销售版本（首版投影也指向切换时的当前版本）。
    pub sales_order_revision_id: SalesOrderRevisionId,
    /// 商城客户标识。
    pub customer_external_identity: String,
    /// 商城卡券类目标识。
    pub voucher_category_external_identity: String,
    /// 表头履约期限（保留来源精确到期时间）。
    pub voucher_expiry_at: Instant,
    /// 卡券面额。
    pub face_value: Amount,
    /// 卡张数。
    pub card_count: u32,
    /// 电子卡或实体卡。
    pub card_form: CardForm,
    /// ERP 生效时间。
    pub effective_at: Instant,
    /// 投影内容指纹（P3 形成版本时计算）。
    pub content_hash: String,
}

/// 执行投影版本实体（不可变版本，数据模型 §6.16）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderProjectionRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 投影稳定身份。
    pub projection_id: SalesOrderProjectionId,
    /// 投影来源。
    pub projection_source: ProjectionSource,
    /// ERP 销售版本（首版投影也指向切换时的当前版本）。
    pub sales_order_revision_id: SalesOrderRevisionId,
    /// 商城客户标识。
    pub customer_external_identity: String,
    /// 商城卡券类目标识。
    pub voucher_category_external_identity: String,
    /// 表头履约期限。
    pub voucher_expiry_at: Instant,
    /// 卡券面额。
    pub face_value: Amount,
    /// 卡张数。
    pub card_count: u32,
    /// 电子卡或实体卡。
    pub card_form: CardForm,
    /// ERP 生效时间。
    pub effective_at: Instant,
    /// 投影内容指纹。
    pub content_hash: String,
}

impl SalesOrderProjectionRevision {
    /// 创建执行投影版本。
    ///
    /// 完成商城客户/卡券类目标识与内容指纹的校验和规范化，并校验唯一明细执行
    /// 字段（面额大于零、卡张数为正，§4.2 卡张数非负整数上限为业务必需正数）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderProjectionRevisionId`）
    /// * `revision_no` - 修订序号（同一投影内从 1 递增）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的投影版本实体。
    ///
    /// # 错误
    /// 当商城标识或内容指纹为空/超长、面额不大于零或卡张数为零时返回错误。
    pub fn new(
        id: SalesOrderProjectionRevisionId,
        revision_no: u32,
        data: SalesOrderProjectionRevisionData,
    ) -> Result<Self> {
        let customer_external_identity = normalize_required_text(
            data.customer_external_identity,
            "商城客户标识不能为空",
            EXTERNAL_IDENTITY_MAX_LEN,
            "商城客户标识过长",
        )?;
        let voucher_category_external_identity = normalize_required_text(
            data.voucher_category_external_identity,
            "商城卡券类目标识不能为空",
            EXTERNAL_IDENTITY_MAX_LEN,
            "商城卡券类目标识过长",
        )?;
        let content_hash = normalize_required_text(
            data.content_hash,
            "投影内容指纹不能为空",
            HASH_MAX_LEN,
            "投影内容指纹过长",
        )?;
        validate_execution_fields(data.face_value, data.card_count)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(revision_no),
            projection_id: data.projection_id,
            projection_source: data.projection_source,
            sales_order_revision_id: data.sales_order_revision_id,
            customer_external_identity,
            voucher_category_external_identity,
            voucher_expiry_at: data.voucher_expiry_at,
            face_value: data.face_value,
            card_count: data.card_count,
            card_form: data.card_form,
            effective_at: data.effective_at,
            content_hash,
        })
    }
}

/// 校验唯一卡券明细执行字段。
///
/// # 参数
/// * `face_value` - 卡券面额
/// * `card_count` - 卡张数
///
/// # 错误
/// 当面额不大于零或卡张数为零时返回错误。
fn validate_execution_fields(face_value: Amount, card_count: u32) -> Result<()> {
    if face_value.to_decimal() <= Decimal::ZERO {
        return Err(Error::from("卡券面额必须大于零"));
    }
    if card_count == 0 {
        return Err(Error::from("卡张数必须大于零"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CardForm, ProjectionSource, SalesOrderProjectionRevision, SalesOrderProjectionRevisionData};
    use crate::common::time::Instant;
    use crate::ids::{SalesOrderProjectionId, SalesOrderProjectionRevisionId, SalesOrderRevisionId};
    use crate::money::Amount;
    use std::str::FromStr;

    fn revision_data() -> SalesOrderProjectionRevisionData {
        SalesOrderProjectionRevisionData {
            projection_id: SalesOrderProjectionId::new("proj-1"),
            projection_source: ProjectionSource::ErpRevision,
            sales_order_revision_id: SalesOrderRevisionId::new("so-rev-1"),
            customer_external_identity: " mall-customer-001 ".to_string(),
            voucher_category_external_identity: " mall-voucher-001 ".to_string(),
            voucher_expiry_at: Instant::from_unix_secs(1_800_000_000),
            face_value: Amount::from_str("100.00").unwrap(),
            card_count: 100,
            card_form: CardForm::Electronic,
            effective_at: Instant::from_unix_secs(1_700_000_000),
            content_hash: " 0011aabbccdd ".to_string(),
        }
    }

    #[test]
    fn revision_new_trims_identities_and_keeps_execution_fields() {
        let revision = SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-1"),
            1,
            revision_data(),
        )
        .unwrap();

        assert_eq!(revision.customer_external_identity, "mall-customer-001");
        assert_eq!(revision.voucher_category_external_identity, "mall-voucher-001");
        assert_eq!(revision.content_hash, "0011aabbccdd");
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.face_value, Amount::from_str("100.00").unwrap());
        assert_eq!(revision.card_count, 100);
        assert_eq!(revision.card_form, CardForm::Electronic);
        assert_eq!(
            revision.sales_order_revision_id,
            SalesOrderRevisionId::new("so-rev-1")
        );
        assert_eq!(revision.projection_source, ProjectionSource::ErpRevision);
        assert_eq!(revision.voucher_expiry_at.unix_secs(), 1_800_000_000);
    }

    #[test]
    fn revision_new_rejects_empty_required_fields() {
        let blank_customer = SalesOrderProjectionRevisionData {
            customer_external_identity: "   ".to_string(),
            ..revision_data()
        };
        assert!(SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-2"),
            1,
            blank_customer
        )
        .is_err());

        let blank_category = SalesOrderProjectionRevisionData {
            voucher_category_external_identity: "  ".to_string(),
            ..revision_data()
        };
        assert!(SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-3"),
            1,
            blank_category
        )
        .is_err());

        let blank_hash = SalesOrderProjectionRevisionData {
            content_hash: " ".to_string(),
            ..revision_data()
        };
        assert!(SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-4"),
            1,
            blank_hash
        )
        .is_err());
    }

    #[test]
    fn revision_new_rejects_overlong_identity_and_hash() {
        let overlong_customer = SalesOrderProjectionRevisionData {
            customer_external_identity: "c".repeat(257),
            ..revision_data()
        };
        assert!(SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-5"),
            1,
            overlong_customer
        )
        .is_err());

        let overlong_hash = SalesOrderProjectionRevisionData {
            content_hash: "h".repeat(129),
            ..revision_data()
        };
        assert!(SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-6"),
            1,
            overlong_hash
        )
        .is_err());
    }

    #[test]
    fn revision_new_rejects_non_positive_face_value_and_zero_card_count() {
        let zero_face = SalesOrderProjectionRevisionData {
            face_value: Amount::from_str("0.00").unwrap(),
            ..revision_data()
        };
        assert!(SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-7"),
            1,
            zero_face
        )
        .is_err());

        let negative_face = SalesOrderProjectionRevisionData {
            face_value: Amount::from_str("-1.00").unwrap(),
            ..revision_data()
        };
        assert!(SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-8"),
            1,
            negative_face
        )
        .is_err());

        let zero_count = SalesOrderProjectionRevisionData {
            card_count: 0,
            ..revision_data()
        };
        assert!(SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-9"),
            1,
            zero_count
        )
        .is_err());
    }

    #[test]
    fn revision_face_value_persists_as_decimal128_on_wire() {
        let revision = SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new("proj-rev-1"),
            1,
            revision_data(),
        )
        .unwrap();

        let bytes = bson::serialize_to_vec(&revision).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert!(matches!(
            wire_doc.get("face_value"),
            Some(bson::Bson::Decimal128(_))
        ));

        let back: SalesOrderProjectionRevision = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back, revision);
    }

    #[test]
    fn revision_enums_serialize_with_stable_codes_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&ProjectionSource::CutoverSnapshot).unwrap(),
            "\"cutover_snapshot\""
        );
        assert_eq!(
            serde_json::to_string(&CardForm::Physical).unwrap(),
            "\"physical\""
        );
        assert_eq!(ProjectionSource::ErpRevision.label(), "ERP 销售版本");
        assert_eq!(CardForm::Electronic.label(), "电子卡");
    }
}
