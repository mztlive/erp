//! 提供方公开实体到采购消费事实的逐字段映射；不做筛选、默认值或资格判断。
use erp_catalog::{Product, ProductCategory, ProductKind as CatalogProductKind, ProductRevision, Sku};
use erp_core::ids::SalesOrderRevisionLineId;
use erp_inventory::{StockBalance, StockReservation};
use erp_procurement::entity::facts::{
    AvailabilityFact, AvailabilityStatus, CurrentRevisionFact, ExistingStockReservationFact, FactIdentity,
    OfferingFact, OfferingRevisionFact, ProductCategoryFact, ProductFact, ProductKind, ProductRevisionFact,
    SalesContractSnapshotFact, SalesCustomerSnapshotFact, SalesGoodsLineFact, SalesLineType,
    SalesOrderBasisFact, SalesRevisionFact, SalesRevisionLineFact, SkuFact, StockBalanceFact,
    SupplierCommercialFact, SupplierRoleFact, VersionedFactIdentity,
};
use erp_procurement::entity::purchase_order::{OfferingDeliveryFact, PurchaseDeliveryAllocation};
use erp_sales::entity::sales_order::{
    CommercialStatus, LineType, SalesOrder, SalesOrderGoodsServiceLineRevision, SalesOrderRevision,
    SalesOrderRevisionLine,
};
use erp_sales::entity::sales_review::SalesChangeSubmissionLine;
use erp_supplier::{SupplierAccount, SupplierCommercialProfileRevision};
use erp_supply::entity::supplier_offering::{
    AvailabilityStatus as SupplyAvailabilityStatus, ResolvedDelivery, SupplierOffering,
    SupplierOfferingAvailability, SupplierOfferingRevision,
};

/// 投影采购依据身份；销售状态资格和 guard CAS 仍由对应调用位置执行。
///
/// # 参数
/// * `order` - 销售单。
///
/// # 返回
/// 返回采购依据身份事实。
///
/// # 错误
/// 不返回错误。
pub fn sales_order_basis_fact(order: &SalesOrder) -> SalesOrderBasisFact {
    SalesOrderBasisFact {
        base: FactIdentity { id: order.base.id.clone() },
        is_effective: order.commercial_status == CommercialStatus::Effective,
        order_no: order.order_no.clone(),
        procurement_guard_version: order.procurement_guard_version,
    }
}
/// 保留选定销售修订的客户和可选合同快照。
///
/// # 参数
/// * `value` - 销售修订。
///
/// # 返回
/// 返回客户名称和可选合同号快照。
///
/// # 错误
/// 不返回错误。
pub(crate) fn sales_revision_fact(value: SalesOrderRevision) -> SalesRevisionFact {
    SalesRevisionFact {
        base: FactIdentity { id: value.base.id },
        customer_snapshot: SalesCustomerSnapshotFact { customer_name: value.customer_snapshot.customer_name },
        contract_snapshot: value
            .contract_snapshot
            .map(|snapshot| SalesContractSnapshotFact { contract_no: snapshot.contract_no }),
    }
}
/// 保留公共销售行身份、类型与显示快照，缺子行仍由采购规则判定。
///
/// # 参数
/// * `value` - 销售版本公共行。
///
/// # 返回
/// 返回公共行身份、类型与显示快照。
///
/// # 错误
/// 不返回错误。
pub(crate) fn sales_line_fact(value: SalesOrderRevisionLine) -> SalesRevisionLineFact {
    SalesRevisionLineFact {
        base: FactIdentity { id: value.base.id },
        sales_order_line_id: value.sales_order_line_id,
        line_no: value.line_no,
        line_type: match value.line_type {
            LineType::GoodsService => SalesLineType::GoodsService,
            LineType::Voucher => SalesLineType::Voucher,
        },
        item_name_snapshot: value.item_name_snapshot,
        spec_snapshot: value.spec_snapshot,
        unit_snapshot: value.unit_snapshot,
    }
}
/// 投影当前商品子行；数量和期限保留原值类型。
///
/// # 参数
/// * `value` - 实物服务子行修订。
///
/// # 返回
/// 返回 SKU、数量、单位和履约期限。
///
/// # 错误
/// 不返回错误。
pub(crate) fn sales_goods_fact(value: SalesOrderGoodsServiceLineRevision) -> SalesGoodsLineFact {
    SalesGoodsLineFact {
        revision_line_id: value.revision_line_id,
        sku_id: value.sku_id,
        sku_revision_id: value.sku_revision_id,
        quantity: value.quantity,
        base_unit_code: value.base_unit_code,
        fulfillment_due_at: value.fulfillment_due_at,
        delivery_allocations: value
            .delivery_allocations
            .into_iter()
            .map(|row| PurchaseDeliveryAllocation {
                recipient: row.recipient,
                phone: row.phone,
                region_code: row.region_code,
                region_name: row.region_name,
                address: row.address,
                quantity: row.quantity,
            })
            .collect(),
        assigned_delivery_allocations: Vec::new(),
    }
}
/// SKU缺失由稀疏映射保留，现存值只保留商品引用。
///
/// # 参数
/// * `value` - 现存 SKU。
///
/// # 返回
/// 返回 SKU 身份及其商品引用。
///
/// # 错误
/// 不返回错误。
pub(crate) fn sku_fact(value: Sku) -> SkuFact {
    SkuFact { base: FactIdentity { id: value.base.id }, product_id: value.product_id }
}
/// 逐项映射商品种类，不依赖枚举序号或字符串解析。
fn product_kind(value: CatalogProductKind) -> ProductKind {
    match value {
        CatalogProductKind::Physical => ProductKind::Physical,
        CatalogProductKind::Virtual => ProductKind::Virtual,
        CatalogProductKind::OfflineService => ProductKind::OfflineService,
        CatalogProductKind::Voucher => ProductKind::Voucher,
    }
}
/// 保留商品当前修订缺失与稳定类型。
///
/// # 参数
/// * `value` - 商品。
///
/// # 返回
/// 返回商品身份、当前修订指针和商品种类。
///
/// # 错误
/// 不返回错误。
pub(crate) fn product_fact(value: Product) -> ProductFact {
    ProductFact {
        base: FactIdentity { id: value.base.id },
        stable: CurrentRevisionFact { current_revision_id: value.stable.current_revision_id },
        product_kind: product_kind(value.product_kind),
    }
}
/// 投影商品当前修订的分类身份。
///
/// # 参数
/// * `value` - 商品修订。
///
/// # 返回
/// 返回分类身份。
///
/// # 错误
/// 不返回错误。
pub(crate) fn product_revision_fact(value: ProductRevision) -> ProductRevisionFact {
    ProductRevisionFact { category_id: value.category_id }
}
/// 保留分类父级 None 与具体身份，环和完整性由采购规则判断。
///
/// # 参数
/// * `value` - 商品分类。
///
/// # 返回
/// 返回分类事实；`parent_category_id` 无父级时为 `None`。
///
/// # 错误
/// 不返回错误。
pub(crate) fn category_fact(value: ProductCategory) -> ProductCategoryFact {
    ProductCategoryFact { parent_category_id: value.parent_category_id }
}
/// 提供方已筛选的现有库存预占事实不计 released 数量。
///
/// # 参数
/// * `value` - 现有库存预占。
///
/// # 返回
/// 返回销售行、预占数量和已消耗数量。
///
/// # 错误
/// 不返回错误。
pub(crate) fn reservation_fact(value: StockReservation) -> ExistingStockReservationFact {
    ExistingStockReservationFact {
        sales_order_line_id: value.sales_order_line_id,
        reserved_quantity: value.reserved_quantity,
        consumed_quantity: value.consumed_quantity,
    }
}
/// 保留余额版本与数量供原创建依据指纹和上限规则消费。
///
/// # 参数
/// * `value` - 库存余额。
///
/// # 返回
/// 返回余额版本、仓库、SKU 和可用数量。
///
/// # 错误
/// 不返回错误。
pub fn stock_balance_fact(value: StockBalance) -> StockBalanceFact {
    StockBalanceFact {
        base: VersionedFactIdentity { id: value.base.id, version: value.base.version },
        warehouse_id: value.warehouse_id,
        sku_id: value.sku_id,
        available_quantity: value.available_quantity,
    }
}
/// 投影已筛选 ACTIVE 供给身份和当前条款指针。
///
/// # 参数
/// * `value` - 提供方已筛选启用状态的供给。
/// # 返回
/// 返回真实身份、归属、版本和当前条款指针，不推断采购选源。
/// # 错误
/// 无；状态及权限资格由调用方证明。
pub fn offering_fact(value: SupplierOffering) -> OfferingFact {
    OfferingFact {
        base: FactIdentity { id: value.base.id },
        version: value.base.version,
        stable: CurrentRevisionFact { current_revision_id: value.stable.current_revision_id },
        sku_id: value.sku_id,
        supplier_id: value.supplier_id,
    }
}
/// 原条款有效期与价格直接映射，由采购规则在原位置判断。
///
/// # 参数
/// * `value` - 精确供给当前指针关联的条款修订。
/// # 返回
/// 返回原始所属供给、版本、有效期和采购价格事实。
/// # 错误
/// 无；当前指针及归属由调用方或领域规则核对。
pub fn offering_revision_fact(value: SupplierOfferingRevision) -> OfferingRevisionFact {
    OfferingRevisionFact {
        base: FactIdentity { id: value.base.id },
        version: value.base.version,
        supplier_offering_id: value.supplier_offering_id,
        valid_from: value.valid_from,
        valid_to: value.valid_to,
        bulk_supply_price_gross: value.bulk_supply_price_gross,
        dropship_supply_price_gross: value.dropship_supply_price_gross,
        input_tax_rate: value.input_tax_rate,
        bulk_minimum_order_quantity: value.bulk_minimum_order_quantity,
    }
}
/// 由供给生效配送政策形成采购消费的配送事实。
///
/// # 参数
/// * `resolved` - 供给领域解析出的生效政策
///
/// # 返回
/// 返回模板名称、版本与政策。
///
/// # 错误
/// 不返回错误。
pub(crate) fn offering_delivery_fact(resolved: ResolvedDelivery) -> OfferingDeliveryFact {
    OfferingDeliveryFact {
        template_name: resolved.template_name,
        template_revision_no: resolved.template_revision_no,
        policy: resolved.policy,
    }
}

/// 可供状态显式映射；所有不可供状态保持采购原拒绝分支。
///
/// # 参数
/// * `value` - 精确供给的当前独立可供事实。
/// # 返回
/// 返回原数量及版本，未提供数量保持空值。
/// # 错误
/// 无；采购是否可用由采购领域判断。
pub fn availability_fact(value: SupplierOfferingAvailability) -> AvailabilityFact {
    AvailabilityFact {
        base: VersionedFactIdentity { id: value.base.id, version: value.base.version },
        supplier_offering_id: value.supplier_offering_id,
        availability_status: match value.availability_status {
            SupplyAvailabilityStatus::Available => AvailabilityStatus::Available,
            SupplyAvailabilityStatus::Unavailable
            | SupplyAvailabilityStatus::Stopped
            | SupplyAvailabilityStatus::Stale => AvailabilityStatus::Unavailable,
        },
        available_quantity: value.available_quantity,
        source_revision_token: value.source_revision_token,
    }
}
/// 供应商角色只保留当前商务资料指针。
///
/// # 参数
/// * `value` - 供应商账号。
///
/// # 返回
/// 返回当前商务资料修订指针。
///
/// # 错误
/// 不返回错误。
pub(crate) fn supplier_role_fact(value: SupplierAccount) -> SupplierRoleFact {
    SupplierRoleFact { current_commercial_profile_revision_id: value.current_commercial_profile_revision_id }
}
/// 由供应商提供方解释历史付款资料，采购仍负责原 NET-30 回退。
///
/// # 参数
/// * `value` - 供应商商务资料修订。
///
/// # 返回
/// 返回有效付款条件代码和业务类别。
///
/// # 错误
/// 不返回错误。
pub(crate) fn supplier_commercial_fact(value: SupplierCommercialProfileRevision) -> SupplierCommercialFact {
    SupplierCommercialFact {
        payment_term_code: value.effective_payment_term_code(),
        business_category: value.effective_business_category(),
    }
}

/// 投影销售变更提交的商品/服务行，作为预判时的拟生效销售行。
///
/// # 参数
/// * `value` - 销售变更提交行
///
/// # 返回
/// 商品/服务行且字段完整时返回公共行与商品子行；卡券行或缺字段时返回 `None`。
///
/// # 错误
/// 无。
pub(crate) fn proposed_line_fact(
    value: SalesChangeSubmissionLine,
) -> Option<(SalesRevisionLineFact, SalesGoodsLineFact)> {
    if value.line_type != LineType::GoodsService {
        return None;
    }
    let goods = SalesGoodsLineFact {
        revision_line_id: SalesOrderRevisionLineId::new(&value.base.id),
        sku_id: value.sku_id?,
        sku_revision_id: value.sku_revision_id?,
        quantity: value.quantity?,
        base_unit_code: value.base_unit_code?,
        fulfillment_due_at: value.fulfillment_due_at?,
        delivery_allocations: value
            .delivery_allocations
            .into_iter()
            .map(|row| PurchaseDeliveryAllocation {
                recipient: row.recipient,
                phone: row.phone,
                region_code: row.region_code,
                region_name: row.region_name,
                address: row.address,
                quantity: row.quantity,
            })
            .collect(),
        assigned_delivery_allocations: Vec::new(),
    };
    let line = SalesRevisionLineFact {
        base: FactIdentity { id: value.base.id },
        sales_order_line_id: value.sales_order_line_id,
        line_no: value.line_no,
        line_type: SalesLineType::GoodsService,
        item_name_snapshot: value.item_name_snapshot,
        spec_snapshot: value.spec_snapshot,
        unit_snapshot: value.unit_snapshot,
    };
    Some((line, goods))
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        SalesChangeSubmissionId, SalesChangeSubmissionLineId, SalesOrderLineId, SkuId, SkuRevisionId,
        SupplierOfferingId,
    };
    use erp_core::region::RegionCode;
    use erp_sales::entity::sales_order::{GoodsLineFields, SalesDeliveryAllocation, SalesPricingMode};
    use erp_sales::entity::sales_review::SalesChangeSubmissionLineData;

    use super::*;

    fn proposed_goods_line() -> SalesChangeSubmissionLine {
        let mut line = SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new("proposed-line"),
            SalesChangeSubmissionId::new("submission"),
            SalesChangeSubmissionLineData {
                sales_order_line_id: SalesOrderLineId::new("sales-line"),
                line_no: 3,
                line_type: LineType::GoodsService,
                sales_tax_rate: "0.13".parse().unwrap(),
                item_name_snapshot: "商品快照".into(),
                spec_snapshot: Some("规格快照".into()),
                unit_snapshot: Some("盒".into()),
                goods: Some(GoodsLineFields {
                    sku_id: SkuId::new("sku"),
                    sku_revision_id: SkuRevisionId::new("sku-revision"),
                    welfare_scenario: None,
                    procurement_supply_price_gross: Some("10".parse().unwrap()),
                    procurement_price_basis: None,
                    fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                    quantity: "2.125".parse().unwrap(),
                    base_unit_code: "BOX".into(),
                    unit_price_gross: "12".parse().unwrap(),
                    pricing_mode: SalesPricingMode::Manual,
                    delivery_allocations: Vec::new(),
                }),
                voucher: None,
            },
        )
        .unwrap();
        line.base = BaseModel::fake();
        line.base.id = "proposed-line".into();
        line.delivery_allocations = ["1.25", "0.875"]
            .into_iter()
            .map(|quantity| SalesDeliveryAllocation {
                recipient: quantity.into(),
                phone: "13800000000".into(),
                region_code: RegionCode::parse_leaf("310101").unwrap(),
                region_name: "上海市黄浦区".into(),
                address: format!("地址 {quantity}"),
                quantity: quantity.parse().unwrap(),
            })
            .collect();
        line
    }

    #[test]
    fn proposed_line_preserves_snapshots_exact_quantities_and_delivery_order() {
        let input = proposed_goods_line();
        let deliveries = input.delivery_allocations.clone();
        let (line, goods) = proposed_line_fact(input).unwrap();
        assert_eq!(line.base.id, "proposed-line");
        assert_eq!(line.sales_order_line_id.as_ref(), "sales-line");
        assert_eq!(line.line_no, 3);
        assert_eq!(line.line_type, SalesLineType::GoodsService);
        assert_eq!(line.item_name_snapshot, "商品快照");
        assert_eq!(line.spec_snapshot.as_deref(), Some("规格快照"));
        assert_eq!(line.unit_snapshot.as_deref(), Some("盒"));
        assert_eq!(goods.revision_line_id.as_ref(), "proposed-line");
        assert_eq!(goods.sku_id.as_ref(), "sku");
        assert_eq!(goods.sku_revision_id.as_ref(), "sku-revision");
        assert_eq!(goods.quantity, "2.125".parse().unwrap());
        assert_eq!(goods.base_unit_code, "BOX");
        assert_eq!(goods.fulfillment_due_at, Instant::from_unix_secs(1_800_000_000));
        assert!(goods.assigned_delivery_allocations.is_empty());
        assert_eq!(goods.delivery_allocations.len(), deliveries.len());
        for (actual, expected) in goods.delivery_allocations.iter().zip(deliveries) {
            assert_eq!(actual.recipient, expected.recipient);
            assert_eq!(actual.phone, expected.phone);
            assert_eq!(actual.region_code, expected.region_code);
            assert_eq!(actual.region_name, expected.region_name);
            assert_eq!(actual.address, expected.address);
            assert_eq!(actual.quantity, expected.quantity);
        }
    }

    #[test]
    fn proposed_line_preserves_empty_delivery_and_missing_field_semantics() {
        let mut empty = proposed_goods_line();
        empty.delivery_allocations.clear();
        assert!(proposed_line_fact(empty).unwrap().1.delivery_allocations.is_empty());
        for missing in 0..6 {
            let mut input = proposed_goods_line();
            match missing {
                0 => input.sku_id = None,
                1 => input.sku_revision_id = None,
                2 => input.quantity = None,
                3 => input.base_unit_code = None,
                4 => input.fulfillment_due_at = None,
                5 => input.line_type = LineType::Voucher,
                _ => unreachable!(),
            }
            assert!(proposed_line_fact(input).is_none(), "missing field {missing}");
        }
    }

    #[test]
    fn availability_preserves_unknown_limits_tokens_and_unavailable_statuses() {
        for (status, expected) in [
            (SupplyAvailabilityStatus::Available, AvailabilityStatus::Available),
            (SupplyAvailabilityStatus::Unavailable, AvailabilityStatus::Unavailable),
            (SupplyAvailabilityStatus::Stopped, AvailabilityStatus::Unavailable),
            (SupplyAvailabilityStatus::Stale, AvailabilityStatus::Unavailable),
        ] {
            for (quantity, token) in [(None, None), (Some("2.125".parse().unwrap()), Some("v2".into()))] {
                let mut base = BaseModel::fake();
                base.id = "availability".into();
                base.version = 7;
                let fact = availability_fact(SupplierOfferingAvailability {
                    base,
                    supplier_offering_id: SupplierOfferingId::new("offering"),
                    availability_status: status,
                    available_quantity: quantity,
                    source_updated_at: Some(Instant::from_unix_secs(1)),
                    received_at: Instant::from_unix_secs(2),
                    verified_at: None,
                    valid_until: None,
                    source_revision_token: token.clone(),
                    updated_by: "operator".into(),
                });
                assert_eq!(fact.base.id, "availability");
                assert_eq!(fact.base.version, 7);
                assert_eq!(fact.supplier_offering_id.as_ref(), "offering");
                assert_eq!(fact.availability_status, expected);
                assert_eq!(fact.available_quantity, quantity);
                assert_eq!(fact.source_revision_token, token);
            }
        }
    }

    /// 四种商品事实必须保持提供方稳定 wire，尤其不得把 OFFLINE_SERVICE 当作普通大写词。
    #[test]
    fn product_kind_fact_keeps_provider_wire() {
        for (source, expected) in [
            (CatalogProductKind::Physical, "PHYSICAL"),
            (CatalogProductKind::Virtual, "VIRTUAL"),
            (CatalogProductKind::OfflineService, "OFFLINE_SERVICE"),
            (CatalogProductKind::Voucher, "VOUCHER"),
        ] {
            let target = product_kind(source);
            assert_eq!(target.as_str(), expected);
            assert_eq!(serde_json::to_value(target).unwrap(), serde_json::json!(expected));
            assert_eq!(serde_json::to_value(source).unwrap(), serde_json::json!(expected));
        }
    }
}
