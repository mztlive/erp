//! 提供方公开实体到采购消费事实的逐字段映射；不做筛选、默认值或资格判断。
use erp_procurement::entity::facts::{
    AvailabilityFact, AvailabilityStatus, CurrentRevisionFact, ExistingStockReservationFact, FactIdentity,
    OfferingFact, OfferingRevisionFact, ProductCategoryFact, ProductFact, ProductKind, ProductRevisionFact,
    SalesContractSnapshotFact, SalesCustomerSnapshotFact, SalesGoodsLineFact, SalesLineType,
    SalesOrderBasisFact, SalesRevisionFact, SalesRevisionLineFact, SkuFact, StockBalanceFact,
    SupplierCommercialFact, SupplierRoleFact, VersionedFactIdentity,
};
use erp_sales::entity::sales_order::{
    CommercialStatus, LineType, SalesOrder, SalesOrderGoodsServiceLineRevision, SalesOrderRevision,
    SalesOrderRevisionLine,
};

/// 投影采购依据身份；销售状态资格和 guard CAS 仍由对应调用位置执行。
pub fn sales_order_basis_fact(order: &SalesOrder) -> SalesOrderBasisFact {
    SalesOrderBasisFact {
        base: FactIdentity {
            id: order.base.id.clone(),
        },
        is_effective: order.commercial_status == CommercialStatus::Effective,
        order_no: order.order_no.clone(),
        procurement_guard_version: order.procurement_guard_version,
    }
}
/// 保留选定销售修订的客户和可选合同快照。
pub(crate) fn sales_revision_fact(value: SalesOrderRevision) -> SalesRevisionFact {
    SalesRevisionFact {
        base: FactIdentity {
            id: value.base.id.clone(),
        },
        customer_snapshot: SalesCustomerSnapshotFact {
            customer_name: value.customer_snapshot.customer_name.clone(),
        },
        contract_snapshot: value
            .contract_snapshot
            .as_ref()
            .map(|snapshot| SalesContractSnapshotFact {
                contract_no: snapshot.contract_no.clone(),
            }),
    }
}
/// 保留公共销售行身份、类型与显示快照，缺子行仍由采购规则判定。
pub(crate) fn sales_line_fact(value: SalesOrderRevisionLine) -> SalesRevisionLineFact {
    SalesRevisionLineFact {
        base: FactIdentity {
            id: value.base.id.clone(),
        },
        sales_order_line_id: value.sales_order_line_id.clone(),
        line_no: value.line_no,
        line_type: match value.line_type {
            LineType::GoodsService => SalesLineType::GoodsService,
            LineType::Voucher => SalesLineType::Voucher,
        },
        item_name_snapshot: value.item_name_snapshot.clone(),
        spec_snapshot: value.spec_snapshot.clone(),
        unit_snapshot: value.unit_snapshot.clone(),
    }
}
/// 投影当前商品子行；数量和期限保留原值类型。
pub(crate) fn sales_goods_fact(value: SalesOrderGoodsServiceLineRevision) -> SalesGoodsLineFact {
    SalesGoodsLineFact {
        revision_line_id: value.revision_line_id.clone(),
        sku_id: value.sku_id.clone(),
        sku_revision_id: value.sku_revision_id.clone(),
        quantity: value.quantity,
        base_unit_code: value.base_unit_code.clone(),
        fulfillment_due_at: value.fulfillment_due_at,
    }
}
/// SKU缺失由稀疏映射保留，现存值只保留商品引用。
pub(crate) fn sku_fact(value: erp_catalog::Sku) -> SkuFact {
    SkuFact {
        base: FactIdentity {
            id: value.base.id.clone(),
        },
        product_id: value.product_id.clone(),
    }
}
/// 逐项映射商品种类，不依赖枚举序号或字符串解析。
fn product_kind(value: erp_catalog::ProductKind) -> ProductKind {
    match value {
        erp_catalog::ProductKind::Physical => ProductKind::Physical,
        erp_catalog::ProductKind::Virtual => ProductKind::Virtual,
        erp_catalog::ProductKind::OfflineService => ProductKind::OfflineService,
        erp_catalog::ProductKind::Voucher => ProductKind::Voucher,
    }
}
/// 保留商品当前修订缺失与稳定类型。
pub(crate) fn product_fact(value: erp_catalog::Product) -> ProductFact {
    ProductFact {
        base: FactIdentity {
            id: value.base.id.clone(),
        },
        stable: CurrentRevisionFact {
            current_revision_id: value.stable.current_revision_id.clone(),
        },
        product_kind: product_kind(value.product_kind),
    }
}
/// 投影商品当前修订的分类身份。
pub(crate) fn product_revision_fact(value: erp_catalog::ProductRevision) -> ProductRevisionFact {
    ProductRevisionFact {
        category_id: value.category_id.clone(),
    }
}
/// 保留分类父级 None 与具体身份，环和完整性由采购规则判断。
pub(crate) fn category_fact(value: erp_catalog::ProductCategory) -> ProductCategoryFact {
    ProductCategoryFact {
        parent_category_id: value.parent_category_id.clone(),
    }
}
/// 提供方已筛选的现有库存预占事实不计 released 数量。
pub(crate) fn reservation_fact(value: erp_inventory::StockReservation) -> ExistingStockReservationFact {
    ExistingStockReservationFact {
        sales_order_line_id: value.sales_order_line_id.clone(),
        reserved_quantity: value.reserved_quantity,
        consumed_quantity: value.consumed_quantity,
    }
}
/// 保留余额版本与数量供原创建依据指纹和上限规则消费。
pub fn stock_balance_fact(value: erp_inventory::StockBalance) -> StockBalanceFact {
    StockBalanceFact {
        base: VersionedFactIdentity {
            id: value.base.id.clone(),
            version: value.base.version,
        },
        warehouse_id: value.warehouse_id.clone(),
        sku_id: value.sku_id.clone(),
        available_quantity: value.available_quantity,
    }
}
/// 投影已筛选 ACTIVE 供给身份和当前条款指针。
pub(crate) fn offering_fact(value: erp_supply::entity::supplier_offering::SupplierOffering) -> OfferingFact {
    OfferingFact {
        base: FactIdentity {
            id: value.base.id.clone(),
        },
        stable: CurrentRevisionFact {
            current_revision_id: value.stable.current_revision_id.clone(),
        },
        sku_id: value.sku_id.clone(),
        supplier_id: value.supplier_id.clone(),
    }
}
/// 原条款有效期与价格直接映射，由采购规则在原位置判断。
pub(crate) fn offering_revision_fact(
    value: erp_supply::entity::supplier_offering::SupplierOfferingRevision,
) -> OfferingRevisionFact {
    OfferingRevisionFact {
        base: FactIdentity {
            id: value.base.id.clone(),
        },
        valid_from: value.valid_from,
        valid_to: value.valid_to,
        bulk_supply_price_gross: value.bulk_supply_price_gross,
        dropship_supply_price_gross: value.dropship_supply_price_gross,
        input_tax_rate: value.input_tax_rate,
    }
}
/// 可供状态显式映射；所有不可供状态保持采购原拒绝分支。
pub(crate) fn availability_fact(
    value: erp_supply::entity::supplier_offering::SupplierOfferingAvailability,
) -> AvailabilityFact {
    AvailabilityFact {
        base: VersionedFactIdentity {
            id: value.base.id.clone(),
            version: value.base.version,
        },
        supplier_offering_id: value.supplier_offering_id.clone(),
        availability_status: match value.availability_status {
            erp_supply::entity::supplier_offering::AvailabilityStatus::Available => {
                AvailabilityStatus::Available
            }
            erp_supply::entity::supplier_offering::AvailabilityStatus::Unavailable
            | erp_supply::entity::supplier_offering::AvailabilityStatus::Stopped
            | erp_supply::entity::supplier_offering::AvailabilityStatus::Stale => {
                AvailabilityStatus::Unavailable
            }
        },
        available_quantity: value.available_quantity,
        source_revision_token: value.source_revision_token.clone(),
    }
}
/// 供应商角色只保留当前商务资料指针。
pub(crate) fn supplier_role_fact(value: erp_supplier::SupplierAccount) -> SupplierRoleFact {
    SupplierRoleFact {
        current_commercial_profile_revision_id: value.current_commercial_profile_revision_id.clone(),
    }
}
/// 由供应商提供方解释历史付款资料，采购仍负责原 NET-30 回退。
pub(crate) fn supplier_commercial_fact(
    value: erp_supplier::SupplierCommercialProfileRevision,
) -> SupplierCommercialFact {
    SupplierCommercialFact {
        payment_term_code: value.effective_payment_term_code(),
        business_category: value.effective_business_category(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 四种商品事实必须保持提供方稳定 wire，尤其不得把 OFFLINE_SERVICE 当作普通大写词。
    #[test]
    fn product_kind_fact_keeps_provider_wire() {
        for (source, expected) in [
            (erp_catalog::ProductKind::Physical, "PHYSICAL"),
            (erp_catalog::ProductKind::Virtual, "VIRTUAL"),
            (erp_catalog::ProductKind::OfflineService, "OFFLINE_SERVICE"),
            (erp_catalog::ProductKind::Voucher, "VOUCHER"),
        ] {
            let target = product_kind(source);
            assert_eq!(target.as_str(), expected);
            assert_eq!(serde_json::to_value(target).unwrap(), serde_json::json!(expected));
            assert_eq!(serde_json::to_value(source).unwrap(), serde_json::json!(expected));
        }
    }
}
