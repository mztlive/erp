//! `purchase_order_submission_line` 采购提交行（数据模型 §6.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    ProcurementConfirmationLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
    SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::purchase_order::line_common::{PurchaseLineFields, normalize_expense};
use crate::entity::purchase_order::offering_source::ensure_offering_source;
use crate::entity::purchase_order::types::PurchaseLineType;
use crate::entity::purchase_order::{
    PurchaseDeliveryAllocation, PurchaseExpenseDetail, PurchaseOfferingSource,
};

/// 采购提交行创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderSubmissionLineData {
    /// 所属提交。
    pub purchase_order_submission_id: PurchaseOrderSubmissionId,
    /// 行号（从 1 递增）。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU；物流费用行为空。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本；物流费用行为空。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 服务端冻结的正式供给选源；旧资料或物流费用行为空。
    #[serde(default)]
    pub supplier_offering_source: Option<PurchaseOfferingSource>,
    /// 本采购行承担的客户交付安排。
    #[serde(default)]
    pub delivery_allocations: Vec<PurchaseDeliveryAllocation>,
    /// 商品名称快照；物流费用行为空。
    pub product_name_snapshot: Option<String>,
    /// 规格快照；物流费用行为空。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量；物流费用行为空。
    pub quantity: Option<Quantity>,
    /// 单位代码；物流费用行为空。
    pub base_unit_code: Option<String>,
    /// 含税采购单价；物流费用行为空。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
    /// 预计交期。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<SalesOrderRevisionLineId>,
    /// 商品行对应的历史销售提交行；仅保留旧流程追溯。
    pub sales_order_submission_line_id: Option<SalesOrderSubmissionLineId>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<Quantity>,
    /// 费用行内容（稳定身份、类别、说明、归属）；商品行为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expense: Option<PurchaseExpenseDetail>,
}

impl PurchaseOrderSubmissionLineData {
    fn take_validated_fields(&mut self) -> Result<PurchaseLineFields> {
        let mut fields = PurchaseLineFields {
            line_type: self.line_type,
            procurement_confirmation_line_id: self.procurement_confirmation_line_id.take(),
            sku_id: self.sku_id.take(),
            product_name_snapshot: self.product_name_snapshot.take(),
            specification_snapshot: self.specification_snapshot.take(),
            quantity: self.quantity,
            base_unit_code: self.base_unit_code.take(),
            unit_cost_gross: self.unit_cost_gross,
            gross_amount: self.gross_amount,
            net_amount: self.net_amount,
            tax_amount: self.tax_amount,
            input_tax_rate: self.input_tax_rate,
        };
        fields.normalize_and_validate(self.line_no, false)?;

        self.ensure_allocation()?;
        ensure_offering_source(
            self.line_type,
            self.supplier_offering_source.as_ref(),
            self.unit_cost_gross,
            self.input_tax_rate,
        )?;
        PurchaseDeliveryAllocation::validate(&mut self.delivery_allocations, self.line_type, self.quantity)?;
        Ok(fields)
    }

    /// 按行类型校验销售分配字段。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 商品行已引用销售稳定行与当前版本行且分配数量为正，或物流费用行不携带销售分配时返回 `Ok(())`。
    ///
    /// # 错误
    /// 商品行缺少销售行引用、分配数量缺失或非正，或物流费用行携带销售行、历史提交行或分配数量时返回领域错误。
    fn ensure_allocation(&self) -> Result<()> {
        match self.line_type {
            PurchaseLineType::ItemService => {
                if self.sales_order_line_id.is_none() || self.sales_order_revision_line_id.is_none() {
                    return Err(Error::from("商品/服务行必须引用销售稳定行与当前版本行"));
                }
                let quantity = self.allocated_quantity.ok_or("商品/服务行必须填写分配数量")?;
                if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
                    return Err(Error::from("商品/服务行分配数量必须为正"));
                }
            },
            PurchaseLineType::LogisticsFee => {
                if self.sales_order_line_id.is_some()
                    || self.sales_order_revision_line_id.is_some()
                    || self.sales_order_submission_line_id.is_some()
                    || self.allocated_quantity.is_some()
                {
                    return Err(Error::from("物流费用行不得携带销售分配"));
                }
            },
        }
        Ok(())
    }
}

/// 采购提交行实体（数据模型 §6.6）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseOrderSubmissionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属提交。
    pub purchase_order_submission_id: PurchaseOrderSubmissionId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 服务端冻结的正式供给选源；旧资料或物流费用行为空。
    #[serde(default)]
    pub supplier_offering_source: Option<PurchaseOfferingSource>,
    /// 本采购行承担的客户交付安排。
    #[serde(default)]
    pub delivery_allocations: Vec<PurchaseDeliveryAllocation>,
    /// 商品名称快照。
    pub product_name_snapshot: Option<String>,
    /// 规格快照。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量。
    pub quantity: Option<Quantity>,
    /// 单位代码。
    pub base_unit_code: Option<String>,
    /// 含税采购单价。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
    /// 预计交期。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<SalesOrderRevisionLineId>,
    /// 商品行对应的历史销售提交行；仅保留旧流程追溯。
    pub sales_order_submission_line_id: Option<SalesOrderSubmissionLineId>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<Quantity>,
    /// 费用行内容（稳定身份、类别、说明、归属）；商品行为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expense: Option<PurchaseExpenseDetail>,
}

impl PurchaseOrderSubmissionLine {
    /// 创建采购提交行。
    ///
    /// 完成快照文本的规范化，并按行类型强制字段归属与金额三元组守恒（§6.6）；
    /// 商品行必须引用销售稳定行与当前版本行，且分配数量必须为正。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::PurchaseOrderSubmissionLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交行实体。
    ///
    /// # 错误
    /// 行号为零、字段归属与行类型不符、快照超长、数量/单价/税率越界或
    /// 金额三元组不守恒时返回错误。
    pub fn new(id: PurchaseOrderSubmissionLineId, mut data: PurchaseOrderSubmissionLineData) -> Result<Self> {
        let fields = data.take_validated_fields()?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_order_submission_id: data.purchase_order_submission_id,
            line_no: data.line_no,
            line_type: fields.line_type,
            procurement_confirmation_line_id: fields.procurement_confirmation_line_id,
            sku_id: fields.sku_id,
            sku_revision_id: data.sku_revision_id,
            supplier_offering_source: data.supplier_offering_source,
            delivery_allocations: data.delivery_allocations,
            product_name_snapshot: fields.product_name_snapshot,
            specification_snapshot: fields.specification_snapshot,
            quantity: fields.quantity,
            base_unit_code: fields.base_unit_code,
            unit_cost_gross: fields.unit_cost_gross,
            gross_amount: fields.gross_amount,
            net_amount: fields.net_amount,
            tax_amount: fields.tax_amount,
            input_tax_rate: fields.input_tax_rate,
            expected_delivery_date: data.expected_delivery_date,
            sales_order_line_id: data.sales_order_line_id,
            sales_order_revision_line_id: data.sales_order_revision_line_id,
            sales_order_submission_line_id: data.sales_order_submission_line_id,
            allocated_quantity: data.allocated_quantity,
            expense: normalize_expense(data.line_type, data.expense)?,
        })
    }

    /// 把草稿行复制到新的冻结提交。
    ///
    /// # 参数
    /// * `id` - 新提交行稳定身份
    /// * `submission_id` - 新正式提交稳定身份
    /// * `draft_line` - 当前草稿行
    ///
    /// # 返回
    /// 返回业务内容与草稿行一致、重新挂接到正式提交的新行。
    ///
    /// # 错误
    /// 草稿行本身不满足当前采购行不变式时返回领域错误。
    pub fn freeze_from_draft(
        id: PurchaseOrderSubmissionLineId,
        submission_id: PurchaseOrderSubmissionId,
        draft_line: &Self,
    ) -> Result<Self> {
        Self::new(
            id,
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: submission_id,
                line_no: draft_line.line_no,
                line_type: draft_line.line_type,
                procurement_confirmation_line_id: draft_line.procurement_confirmation_line_id.clone(),
                sku_id: draft_line.sku_id.clone(),
                sku_revision_id: draft_line.sku_revision_id.clone(),
                supplier_offering_source: draft_line.supplier_offering_source.clone(),
                delivery_allocations: draft_line.delivery_allocations.clone(),
                product_name_snapshot: draft_line.product_name_snapshot.clone(),
                specification_snapshot: draft_line.specification_snapshot.clone(),
                quantity: draft_line.quantity,
                base_unit_code: draft_line.base_unit_code.clone(),
                unit_cost_gross: draft_line.unit_cost_gross,
                gross_amount: draft_line.gross_amount,
                net_amount: draft_line.net_amount,
                tax_amount: draft_line.tax_amount,
                input_tax_rate: draft_line.input_tax_rate,
                expected_delivery_date: draft_line.expected_delivery_date,
                sales_order_line_id: draft_line.sales_order_line_id.clone(),
                sales_order_revision_line_id: draft_line.sales_order_revision_line_id.clone(),
                sales_order_submission_line_id: draft_line.sales_order_submission_line_id.clone(),
                allocated_quantity: draft_line.allocated_quantity,
                expense: draft_line.expense.clone(),
            },
        )
    }

    /// 校验客户端没有改写服务端冻结的销售与 SKU 来源引用。
    ///
    /// # 参数
    /// * `requested` - 待保存请求行的草稿编辑请求
    ///
    /// # 返回
    /// 所有来源引用一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一稳定身份或版本引用变化时返回
    /// [`DraftLineEditViolation::RewrittenSourceReference`]。
    pub fn ensure_source_references_unchanged(
        &self,
        requested: &super::draft_edit::DraftLineEdit,
    ) -> std::result::Result<(), super::draft_edit::DraftLineEditViolation> {
        let unchanged = [
            (
                requested.procurement_confirmation_line_id.as_deref(),
                self.procurement_confirmation_line_id.as_ref().map(ToString::to_string),
            ),
            (requested.sku_id.as_deref(), self.sku_id.as_ref().map(ToString::to_string)),
            (requested.sku_revision_id.as_deref(), self.sku_revision_id.as_ref().map(ToString::to_string)),
            (
                requested.sales_order_line_id.as_deref(),
                self.sales_order_line_id.as_ref().map(ToString::to_string),
            ),
            (
                requested.sales_order_revision_line_id.as_deref(),
                self.sales_order_revision_line_id.as_ref().map(ToString::to_string),
            ),
            (
                requested.sales_order_submission_line_id.as_deref(),
                self.sales_order_submission_line_id.as_ref().map(ToString::to_string),
            ),
        ]
        .into_iter()
        .all(|(requested, existing)| super::draft_edit::normalized_optional_id(requested) == existing);
        if !unchanged {
            return Err(super::draft_edit::DraftLineEditViolation::RewrittenSourceReference);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::Error;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{
        ProcurementConfirmationLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
        SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice, line_amounts};

    use super::{PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData};
    use crate::entity::purchase_order::types::PurchaseLineType;
    use crate::entity::purchase_order::{
        PurchaseDeliveryAllocation, PurchaseExpenseCategory, PurchaseExpenseDetail,
    };

    fn goods_line_data() -> PurchaseOrderSubmissionLineData {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: PurchaseOrderSubmissionId::new("sub-1"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(erp_core::ids::SkuRevisionId::new("skur-1")),
            supplier_offering_source: None,
            delivery_allocations: Vec::new(),
            product_name_snapshot: Some(" 慰问礼包 ".to_string()),
            specification_snapshot: Some(" 500g×2 ".to_string()),
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            base_unit_code: Some(" 箱 ".to_string()),
            unit_cost_gross: Some(UnitPrice::from_str("9.9900").unwrap()),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
            sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("ssl-1")),
            allocated_quantity: Some(Quantity::from_str("3.000000").unwrap()),
            expense: None,
        }
    }

    /// 完整服务端选源用于冻结和撤回恢复测试。
    fn offering_source() -> super::PurchaseOfferingSource {
        super::PurchaseOfferingSource {
            supplier_offering_id: erp_core::ids::SupplierOfferingId::new("offering-selected"),
            offering_version: 7,
            supplier_offering_revision_id: erp_core::ids::SupplierOfferingRevisionId::new("terms-selected"),
            revision_version: 3,
            availability_version: 11,
            inquiry_quote: None,
        }
    }

    #[test]
    fn selected_source_survives_freeze_formalization_change_and_client_patch() {
        use erp_core::ids::{
            PurchaseChangeSubmissionId, PurchaseChangeSubmissionLineId, PurchaseOrderRevisionId,
            PurchaseOrderRevisionLineId,
        };

        use crate::dto::purchase_order::{SavePurchaseOrderLine, SavePurchaseOrderLinePatch};
        use crate::entity::purchase_order::{
            PurchaseOrderRevisionLine, inherit_revision_sources, inherit_submission_sources,
        };

        let mut data = goods_line_data();
        data.supplier_offering_source = Some(offering_source());
        data.delivery_allocations = vec![PurchaseDeliveryAllocation {
            recipient: "张三".into(),
            phone: "13800000000".into(),
            region_code: erp_core::region::RegionCode::parse("310115").unwrap(),
            region_name: "上海市浦东新区".into(),
            address: "客户地址".into(),
            quantity: data.quantity.unwrap(),
        }];
        let draft = PurchaseOrderSubmissionLine::new("draft-line".to_string().into(), data).unwrap();
        let formal = PurchaseOrderSubmissionLine::freeze_from_draft(
            "formal-line".to_string().into(),
            "formal".to_string().into(),
            &draft,
        )
        .unwrap();
        let restored = PurchaseOrderSubmissionLine::freeze_from_draft(
            "restored-line".to_string().into(),
            "restored".to_string().into(),
            &formal,
        )
        .unwrap();
        let revision = PurchaseOrderRevisionLine::from_submission_line(
            PurchaseOrderRevisionLineId::new("revision-line"),
            PurchaseOrderRevisionId::new("revision"),
            &formal,
        )
        .unwrap();
        assert_eq!(draft.supplier_offering_source, restored.supplier_offering_source);
        assert_eq!(draft.supplier_offering_source, revision.supplier_offering_source);
        assert_eq!(draft.delivery_allocations, restored.delivery_allocations);
        assert_eq!(draft.delivery_allocations, revision.delivery_allocations);

        let patch = SavePurchaseOrderLinePatch {
            line_id: draft.base.id.clone(),
            line_type: draft.line_type,
            quantity: Some("2".into()),
            unit_cost_gross: None,
            input_tax_rate: None,
            expected_delivery_date: None,
            delivery_allocations: None,
        };
        let requested =
            SavePurchaseOrderLinePatch::resolve_all(&[patch], std::slice::from_ref(&draft)).unwrap();
        let mut json = serde_json::to_value(&requested[0]).unwrap();
        json["supplier_offering_source"] = serde_json::json!({ "supplier_offering_id": "forged", "supplier_offering_revision_id": "forged-terms", "offering_version": 1, "revision_version": 1, "availability_version": 1 });
        let requested: SavePurchaseOrderLine = serde_json::from_value(json).unwrap();
        let mut inputs = vec![requested.to_line_input().unwrap()];
        assert!(inputs[0].supplier_offering_source.is_none());
        inherit_submission_sources(&mut inputs, std::slice::from_ref(&draft)).unwrap();
        assert_eq!(inputs[0].supplier_offering_source, Some(offering_source()));
        assert_eq!(inputs[0].delivery_allocations, draft.delivery_allocations);

        inputs[0].delivery_allocations[0].quantity = Quantity::from_str("2").unwrap();
        let requested = inputs[0].delivery_allocations.clone();
        inherit_submission_sources(&mut inputs, std::slice::from_ref(&draft)).unwrap();
        assert_eq!(inputs[0].delivery_allocations, requested);
        inherit_revision_sources(&mut inputs, std::slice::from_ref(&revision)).unwrap();

        let before = inputs.clone();
        assert!(matches!(
            inherit_submission_sources(&mut inputs, &[draft.clone(), draft.clone()]),
            Err(Error::LogicError(message)) if message == "采购原行身份歧义，不能继承正式选源"
        ));
        assert_eq!(inputs, before);

        let mut incomplete = draft.clone();
        incomplete.sku_id = None;
        assert!(matches!(
            inherit_submission_sources(&mut inputs, &[draft.clone(), incomplete]),
            Err(Error::LogicError(message)) if message == "采购来源缺少SKU"
        ));
        assert_eq!(inputs, before);

        let mut repeated = vec![inputs[0].clone(), inputs[0].clone()];
        inherit_submission_sources(&mut repeated, std::slice::from_ref(&draft)).unwrap();
        assert_eq!(repeated, vec![before[0].clone(), before[0].clone()]);

        let mut wrong_revision = inputs.clone();
        wrong_revision[0].sku_revision_id = Some(erp_core::ids::SkuRevisionId::new("foreign-version"));
        assert!(inherit_revision_sources(&mut wrong_revision, std::slice::from_ref(&revision)).is_err());

        let change_data = inputs
            .remove(0)
            .into_change_submission_line_data(PurchaseChangeSubmissionId::new("change"), 1)
            .unwrap();
        let change = crate::entity::purchase_order::PurchaseChangeSubmissionLine::new(
            PurchaseChangeSubmissionLineId::new("change-line"),
            change_data,
        )
        .unwrap();
        let changed_revision = PurchaseOrderRevisionLine::from_change_submission_line(
            PurchaseOrderRevisionLineId::new("changed-line"),
            PurchaseOrderRevisionId::new("changed-revision"),
            &change,
        )
        .unwrap();
        assert_eq!(changed_revision.supplier_offering_source, Some(offering_source()));
        assert_eq!(changed_revision.quantity, Some(Quantity::from_str("2").unwrap()));
        assert_eq!(changed_revision.delivery_allocations[0].quantity, Quantity::from_str("2").unwrap());
        assert_eq!(changed_revision.delivery_allocations[0].address, "客户地址");
    }

    #[test]
    fn old_unassociated_lines_stay_unknown_and_invalid_source_is_rejected() {
        let old = PurchaseOrderSubmissionLine::new("old".to_string().into(), goods_line_data()).unwrap();
        let mut json = serde_json::to_value(old).unwrap();
        json.as_object_mut().unwrap().remove("supplier_offering_source");
        json.as_object_mut().unwrap().remove("delivery_allocations");
        let restored: PurchaseOrderSubmissionLine = serde_json::from_value(json).unwrap();
        assert!(restored.supplier_offering_source.is_none());
        assert!(restored.delivery_allocations.is_empty());

        let mut data = goods_line_data();
        let mut invalid = offering_source();
        invalid.revision_version = 0;
        data.supplier_offering_source = Some(invalid);
        assert!(PurchaseOrderSubmissionLine::new("invalid".to_string().into(), data).is_err());
    }

    #[test]
    fn submission_line_goods_happy_path() {
        let line =
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-1"), goods_line_data())
                .unwrap();
        assert_eq!(line.product_name_snapshot.as_deref(), Some("慰问礼包"));
        assert_eq!(line.base_unit_code.as_deref(), Some("箱"));
        assert_eq!(line.line_type, PurchaseLineType::ItemService);
    }

    #[test]
    fn submission_line_normalizes_snapshots_and_accepts_largest_line_no() {
        let data = PurchaseOrderSubmissionLineData {
            line_no: u32::MAX,
            product_name_snapshot: Some(" 礼包 ".into()),
            specification_snapshot: Some(" \t ".into()),
            base_unit_code: Some(" 件 ".into()),
            ..goods_line_data()
        };
        let line = PurchaseOrderSubmissionLine::new("normalized".to_string().into(), data).unwrap();
        assert_eq!(line.line_no, u32::MAX);
        assert_eq!(line.product_name_snapshot.as_deref(), Some("礼包"));
        assert_eq!(line.specification_snapshot, None);
        assert_eq!(line.base_unit_code.as_deref(), Some("件"));
    }

    #[test]
    fn submission_line_preserves_validation_error_precedence() {
        let assert_error = |data, expected: &str| {
            let error = PurchaseOrderSubmissionLine::new("invalid".to_string().into(), data).unwrap_err();
            assert!(matches!(error, Error::LogicError(message) if message == expected));
        };

        let mut data = PurchaseOrderSubmissionLineData {
            line_no: 0,
            product_name_snapshot: Some("名".repeat(257)),
            specification_snapshot: Some("规".repeat(513)),
            base_unit_code: Some("件".repeat(65)),
            sku_id: None,
            sales_order_revision_line_id: None,
            ..goods_line_data()
        };
        assert_error(data.clone(), "行号必须从 1 开始");
        data.line_no = 1;
        assert_error(data.clone(), "商品名称长度不符合要求");
        data.product_name_snapshot = None;
        assert_error(data.clone(), "商品规格长度不符合要求");
        data.specification_snapshot = None;
        assert_error(data.clone(), "单位长度不符合要求");
        data.base_unit_code = Some("件".into());

        assert_error(data.clone(), "商品/服务行必须引用 SKU");
        data.sku_id = Some(SkuId::new("sku-1"));
        assert_error(data.clone(), "请填写商品或服务明细的名称");
        data.product_name_snapshot = Some("礼包".into());
        assert_error(data, "商品/服务行必须引用销售稳定行与当前版本行");
    }

    #[test]
    fn submission_checks_allocation_source_delivery_and_expense_in_order() {
        let assert_error = |data, expected: &str| {
            let error = PurchaseOrderSubmissionLine::new("invalid".to_string().into(), data).unwrap_err();
            assert!(matches!(error, Error::LogicError(message) if message == expected));
        };

        let mut data = goods_line_data();
        data.sales_order_revision_line_id = None;
        let mut source = offering_source();
        source.revision_version = 0;
        data.supplier_offering_source = Some(source);
        data.delivery_allocations = vec![PurchaseDeliveryAllocation {
            recipient: String::new(),
            phone: "13800000000".into(),
            region_code: erp_core::region::RegionCode::parse("310115").unwrap(),
            region_name: "旧地区".into(),
            address: "客户地址".into(),
            quantity: data.quantity.unwrap(),
        }];
        data.expense = Some(
            PurchaseExpenseDetail::new(
                "expense-1".into(),
                PurchaseExpenseCategory::Logistics,
                "运费".into(),
                None,
            )
            .unwrap(),
        );

        assert_error(data.clone(), "商品/服务行必须引用销售稳定行与当前版本行");
        data.sales_order_revision_line_id = Some(SalesOrderRevisionLineId::new("sorl-1"));
        assert_error(data.clone(), "采购正式选源身份或版本缺失");
        data.supplier_offering_source = Some(offering_source());
        assert_error(data.clone(), "交付安排须填写有效收货人、电话、地址和正数数量");
        data.delivery_allocations[0].recipient = "张三".into();
        assert_error(data.clone(), "商品/服务行不得携带费用内容");

        data.expense = None;
        let line = PurchaseOrderSubmissionLine::new("valid".to_string().into(), data).unwrap();
        assert_eq!(line.delivery_allocations[0].region_name, "上海市浦东新区");
    }

    #[test]
    fn submission_line_requires_positive_quantity_and_allocation() {
        for value in ["0", "-1"] {
            let quantity = Quantity::from_str(value).unwrap();
            let data = PurchaseOrderSubmissionLineData { quantity: Some(quantity), ..goods_line_data() };
            let error = PurchaseOrderSubmissionLine::new("quantity".to_string().into(), data).unwrap_err();
            assert!(matches!(error, Error::LogicError(message) if message == "商品/服务行数量必须为正"));
            let data =
                PurchaseOrderSubmissionLineData { allocated_quantity: Some(quantity), ..goods_line_data() };
            let error = PurchaseOrderSubmissionLine::new("allocation".to_string().into(), data).unwrap_err();
            assert!(matches!(error, Error::LogicError(message) if message == "商品/服务行分配数量必须为正"));
        }
    }

    /// 规格可空不影响 SKU、商品名称和单位的必填约束。
    #[test]
    fn unspecified_goods_still_require_identity_name_and_unit() {
        let mut missing_sku = goods_line_data();
        missing_sku.sku_id = None;
        let mut missing_name = goods_line_data();
        missing_name.product_name_snapshot = Some("  ".into());
        let mut missing_unit = goods_line_data();
        missing_unit.base_unit_code = None;

        for (mut data, expected) in [
            (missing_sku, "商品/服务行必须引用 SKU"),
            (missing_name, "请填写商品或服务明细的名称"),
            (missing_unit, "请填写商品或服务明细的计量单位"),
        ] {
            data.specification_snapshot = None;
            let error = PurchaseOrderSubmissionLine::new("invalid".to_string().into(), data).unwrap_err();
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn submission_line_logistics_fee_amounts_consistent() {
        let gross = Amount::from_str("50.00").unwrap();
        let tax = Amount::from_str("6.50").unwrap();
        let net = Amount::from_str("43.50").unwrap();
        let data = PurchaseOrderSubmissionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            supplier_offering_source: None,
            delivery_allocations: Vec::new(),
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: None,
            ..goods_line_data()
        };
        let line = PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-2"), data.clone())
            .unwrap();
        assert_eq!(line.quantity, None);
        assert_eq!(line.gross_amount, gross);

        let data = PurchaseOrderSubmissionLineData {
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("legacy")),
            ..data
        };
        let error = PurchaseOrderSubmissionLine::new("invalid".to_string().into(), data).unwrap_err();
        assert!(matches!(error, Error::LogicError(message) if message == "物流费用行不得携带销售分配"));
    }

    #[test]
    fn submission_line_rejects_failures() {
        // 商品行缺少销售当前版本行
        let no_allocation =
            PurchaseOrderSubmissionLineData { sales_order_revision_line_id: None, ..goods_line_data() };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-3"), no_allocation,)
                .is_err()
        );

        // 物流费用行携带 SKU
        let fee_with_sku = PurchaseOrderSubmissionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            sku_id: Some(SkuId::new("sku-1")),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-4"), fee_with_sku)
                .is_err()
        );

        // 商品行金额三元组不守恒
        let bad_amounts = PurchaseOrderSubmissionLineData {
            gross_amount: Amount::from_str("30.00").unwrap(),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-5"), bad_amounts)
                .is_err()
        );

        // 行号为零
        let zero_line = PurchaseOrderSubmissionLineData { line_no: 0, ..goods_line_data() };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-6"), zero_line).is_err()
        );

        // 超长规格快照
        let overlong = PurchaseOrderSubmissionLineData {
            specification_snapshot: Some("s".repeat(513)),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-7"), overlong).is_err()
        );
    }
}
