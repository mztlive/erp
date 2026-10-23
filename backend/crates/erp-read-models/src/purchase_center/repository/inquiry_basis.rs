//! 列表展示与提交重验共用询价来源装载及采购依据组装。
use erp_procurement::entity::facts::SalesOrderBasisFact;
use erp_procurement::entity::purchase_order::{BasisGroup, CreationBasisFacts, SalesProcurementCoverage};
use erp_procurement::service::purchase_order::creation_basis::{
    InquiryBasisCandidate, basis_groups_from_facts, inquiry_basis_groups,
};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::sales_order::inquiry_source::inquiry_sources;
use mongodb::Database;
use persistence_core::Executor;

use super::inquiry_quotes::inquiry_candidates;
use crate::Result;

/// 一张已授权销售单的询价来源；没有可用报价时仍保留来源身份。
#[derive(Default)]
pub(in crate::purchase_center) struct InquiryBasis {
    source_id: Option<String>,
    candidates: Vec<InquiryBasisCandidate>,
}

impl InquiryBasis {
    /// 每张销售单读取一次来源及报价，供该单全部任务复用。
    /// # 参数
    /// `db` 为数据库，`order` 为已授权销售单，`executor` 为当前查询或写事务。
    /// # 返回
    /// 返回来源及其准确反馈版本的报价候选。
    /// # 错误
    /// 来源、报价或余额读取失败时返回错误，不降级成无询价来源。
    pub(in crate::purchase_center) async fn load(
        db: &Database,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        let Some(source) = inquiry_sources(db).find_by_id(&order.base.id, executor).await? else {
            return Ok(Self::default());
        };
        let candidates = inquiry_candidates(db, &source, executor).await?;
        Ok(Self { source_id: (!source.released).then_some(source.inquiry_id), candidates })
    }

    /// 同时构造通用与专项采购依据，并设置本销售单的改选要求。
    /// # 参数
    /// `order`、`coverage` 为当前销售及覆盖；`scope` 为任务责任行；`facts` 为批量供给。
    /// # 返回
    /// 返回按任务范围限制的通用及有效询价专项方案。
    /// # 错误
    /// 供给、数量或专项条款非法时返回错误。
    pub(in crate::purchase_center) fn groups(
        &self,
        order: &SalesOrderBasisFact,
        coverage: &SalesProcurementCoverage,
        scope: &[String],
        facts: &mut CreationBasisFacts,
    ) -> Result<Vec<BasisGroup>> {
        facts.source_inquiry_id.clone_from(&self.source_id);
        let mut groups = basis_groups_from_facts(order, coverage, scope, facts)?;
        groups.extend(inquiry_basis_groups(order, coverage, scope, facts, &self.candidates)?);
        Ok(groups)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_procurement::entity::facts::{
        AvailabilityFact, AvailabilityStatus, CurrentRevisionFact, FactIdentity, OfferingFact,
        OfferingRevisionFact, ProductKind, SalesCustomerSnapshotFact, SalesGoodsLineFact, SalesLineType,
        SalesRevisionFact, SalesRevisionLineFact, VersionedFactIdentity,
    };
    use erp_procurement::entity::purchase_order::{
        ProcurementCoverageSummary, PurchaseInquiryQuoteSource, SalesProcurementCoverageLine,
        SupplyPriceBasis,
    };

    use super::*;

    fn id(value: &str) -> FactIdentity {
        FactIdentity { id: value.into() }
    }

    fn coverage() -> SalesProcurementCoverage {
        let summary = ProcurementCoverageSummary::new("10".parse().unwrap(), "0".parse().unwrap()).unwrap();
        SalesProcurementCoverage {
            revision: SalesRevisionFact {
                base: id("sales-revision"),
                customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".into() },
                contract_snapshot: None,
            },
            lines: vec![SalesProcurementCoverageLine {
                quantity_scale: Some(0),
                revision_line: SalesRevisionLineFact {
                    base: id("revision-line"),
                    sales_order_line_id: "sales-line".to_string().into(),
                    line_no: 1,
                    line_type: SalesLineType::GoodsService,
                    item_name_snapshot: "商品".into(),
                    spec_snapshot: None,
                    unit_snapshot: Some("件".into()),
                },
                goods_line: SalesGoodsLineFact {
                    revision_line_id: "revision-line".to_string().into(),
                    sku_id: "sku".to_string().into(),
                    sku_revision_id: "sku-revision".to_string().into(),
                    quantity: "10".parse().unwrap(),
                    base_unit_code: "件".into(),
                    fulfillment_due_at: Instant::from_unix_secs(4_102_444_800),
                    delivery_allocations: vec![],
                    assigned_delivery_allocations: vec![],
                },
                product_kind: ProductKind::Virtual,
                summary,
            }],
            summary,
        }
    }

    fn facts() -> CreationBasisFacts {
        CreationBasisFacts {
            offerings: vec![OfferingFact {
                base: id("offering"),
                version: 1,
                stable: CurrentRevisionFact { current_revision_id: Some("offering-revision".into()) },
                sku_id: "sku".to_string().into(),
                supplier_id: "supplier".to_string().into(),
            }],
            revisions: [(
                "offering-revision".into(),
                OfferingRevisionFact {
                    base: id("offering-revision"),
                    version: 1,
                    supplier_offering_id: "offering".to_string().into(),
                    valid_from: "2000-01-01".parse().unwrap(),
                    valid_to: None,
                    bulk_supply_price_gross: "60".parse().unwrap(),
                    dropship_supply_price_gross: "60".parse().unwrap(),
                    input_tax_rate: "0.13".parse().unwrap(),
                    bulk_minimum_order_quantity: "1".parse().unwrap(),
                },
            )]
            .into(),
            availabilities: [(
                "offering".into(),
                AvailabilityFact {
                    base: VersionedFactIdentity { id: "availability".into(), version: 1 },
                    supplier_offering_id: "offering".to_string().into(),
                    availability_status: AvailabilityStatus::Available,
                    available_quantity: None,
                    source_revision_token: None,
                },
            )]
            .into(),
            ..CreationBasisFacts::default()
        }
    }

    fn inquiry() -> InquiryBasis {
        InquiryBasis {
            source_id: Some("inquiry".into()),
            candidates: vec![InquiryBasisCandidate {
                sales_order_line_id: "sales-line".into(),
                supplier_offering_id: "offering".to_string().into(),
                remaining_quote_quantity: "7".parse().unwrap(),
                remaining_address_limits: vec![],
                source: PurchaseInquiryQuoteSource {
                    inquiry_id: "inquiry".into(),
                    requirement_version: 2,
                    feedback_round: 3,
                    inquiry_line_id: "demand".into(),
                    quote_id: "quote".into(),
                    quote_revision_id: "quote-revision".into(),
                    quote_revision_no: 1,
                    quote_version: 1,
                    unit_cost_gross: "45".parse().unwrap(),
                    input_tax_rate: "0.06".parse().unwrap(),
                    available_quantity: "10".parse().unwrap(),
                    address_limits: vec![],
                    expected_delivery_date: BusinessDate::today(),
                    valid_to: None,
                },
            }],
        }
    }

    fn order() -> SalesOrderBasisFact {
        SalesOrderBasisFact {
            is_effective: true,
            base: id("sales"),
            order_no: "XS001".into(),
            procurement_guard_version: 1,
        }
    }

    #[test]
    fn list_and_command_groups_preserve_special_cost_balance_and_reselection() {
        let mut facts = facts();
        let groups = inquiry().groups(&order(), &coverage(), &["sales-line".into()], &mut facts).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(facts.source_inquiry_id.as_deref(), Some("inquiry"));
        let special = groups.iter().find(|group| group.scope.inquiry_quote_revision_id.is_some()).unwrap();
        assert_eq!(special.lines[0].unit_cost(SupplyPriceBasis::Dropship), "45".parse().unwrap());
        assert_eq!(special.lines[0].max_create_quantity, "7".parse().unwrap());
        assert_eq!(special.lines[0].tax_rate(), "0.06".parse().unwrap());
        let general = groups.iter().find(|group| group.scope.inquiry_quote_revision_id.is_none()).unwrap();
        assert_eq!(general.lines[0].unit_cost(SupplyPriceBasis::Dropship), "60".parse().unwrap());
        let groups = InquiryBasis::default()
            .groups(&order(), &coverage(), &["sales-line".into()], &mut facts)
            .unwrap();
        assert_eq!(groups.len(), 1);
        assert!(facts.source_inquiry_id.is_none(), "另一张无询价销售单不能继承上张单的询价关联标记");
    }

    #[test]
    fn legacy_expiry_does_not_remove_quote_or_expand_task_scope() {
        let mut inquiry = inquiry();
        let mut facts = facts();
        let coverage = coverage();
        assert!(inquiry.groups(&order(), &coverage, &["other-line".into()], &mut facts).unwrap().is_empty());
        inquiry.candidates[0].source.valid_to = Some(BusinessDate::today());
        let groups = inquiry.groups(&order(), &coverage, &["sales-line".into()], &mut facts).unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().any(|group| group.scope.inquiry_quote_revision_id.is_some()));
        assert_eq!(facts.source_inquiry_id.as_deref(), Some("inquiry"));
        inquiry.candidates.clear();
        let groups = inquiry.groups(&order(), &coverage, &["sales-line".into()], &mut facts).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(facts.source_inquiry_id.as_deref(), Some("inquiry"), "当前轮次无内部报价仍保留询价关联");
    }
}
