//! 采购消费的外域最小事实。
//!
//! 提供方实体由 Process/Read Model 显式投影到本模块；事实不负责访问数据库，
//! 不用于替代提供方的状态校验、权限判断或事务内 CAS。

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    ProductCategoryId, ProductId, SalesOrderLineId, SalesOrderRevisionLineId, SkuId, SkuRevisionId,
    SupplierAccountId, SupplierCommercialProfileRevisionId, SupplierOfferingId, WarehouseId,
};
use erp_core::money::{Quantity, Rate, UnitPrice};
use serde::{Deserialize, Serialize};

use crate::entity::purchase_order::PurchaseDeliveryAllocation;

/// 消费事实的稳定主键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactIdentity {
    /// 提供方持久化主键原值。
    pub id: String,
}

/// 依据指纹需要的提供方主键与版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedFactIdentity {
    /// 提供方持久化主键原值。
    pub id: String,
    /// 提供方乐观锁版本。
    pub version: u64,
}

/// 提供方当前修订指针，缺失保留原领域首错。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CurrentRevisionFact {
    /// 当前修订主键。
    pub current_revision_id: Option<String>,
}

/// 采购依据身份和展示所需销售稳定事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesOrderBasisFact {
    /// 销售当前是否已生效，由销售提供方解释。
    pub is_effective: bool,
    /// 销售稳定身份。
    pub base: FactIdentity,
    /// 销售业务单号。
    pub order_no: String,
    /// 销售侧供给创建防并发版本。
    pub procurement_guard_version: u64,
}

/// 已选销售修订中的客户名称快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesCustomerSnapshotFact {
    /// 已冻结客户名称。
    pub customer_name: String,
}

/// 已选销售修订中的合同号快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesContractSnapshotFact {
    /// 已冻结合同业务单号。
    pub contract_no: String,
}

/// 采购依据需要的已选销售修订头事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesRevisionFact {
    /// 销售修订身份。
    pub base: FactIdentity,
    /// 已选修订客户快照。
    pub customer_snapshot: SalesCustomerSnapshotFact,
    /// 已选修订合同快照；缺失保持 None。
    pub contract_snapshot: Option<SalesContractSnapshotFact>,
}

/// 采购消费的销售行类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesLineType {
    /// 商品或服务目标。
    GoodsService,
    /// 卡券行，不进入采购目标。
    Voucher,
}

/// 销售当前修订公共行的采购消费事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesRevisionLineFact {
    /// 当前修订行身份。
    pub base: FactIdentity,
    /// 跨修订稳定销售行身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 修订内行号。
    pub line_no: u32,
    /// 销售行类别。
    pub line_type: SalesLineType,
    /// 已选修订商品名称快照。
    pub item_name_snapshot: String,
    /// 已选修订规格快照。
    pub spec_snapshot: Option<String>,
    /// 已选修订单位快照。
    pub unit_snapshot: Option<String>,
}

/// 销售当前修订商品子行的采购消费事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesGoodsLineFact {
    /// 对应销售当前公共修订行。
    pub revision_line_id: SalesOrderRevisionLineId,
    /// 精确 SKU 身份。
    pub sku_id: SkuId,
    /// 已选 SKU 修订身份。
    pub sku_revision_id: SkuRevisionId,
    /// 销售目标数量。
    pub quantity: Quantity,
    /// 已选修订基础单位代码。
    pub base_unit_code: String,
    /// 销售履约期限。
    pub fulfillment_due_at: Instant,
    /// 当前销售版本的交付要求。
    pub delivery_allocations: Vec<PurchaseDeliveryAllocation>,
    /// 当前有效采购已经承担的地址数量。
    pub assigned_delivery_allocations: Vec<PurchaseDeliveryAllocation>,
}

/// 采购使用的商品业务类型，wire 与提供方一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProductKind {
    /// 实物商品。
    Physical,
    /// 虚拟商品。
    Virtual,
    /// 线下服务。
    #[serde(rename = "OFFLINE_SERVICE")]
    OfflineService,
    /// 卡券。
    Voucher,
}

impl ProductKind {
    /// 返回稳定代码，用于既有采购责任选择器。
    ///
    /// 返回 `PHYSICAL`、`VIRTUAL`、`OFFLINE_SERVICE` 或 `VOUCHER`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Physical => "PHYSICAL",
            Self::Virtual => "VIRTUAL",
            Self::OfflineService => "OFFLINE_SERVICE",
            Self::Voucher => "VOUCHER",
        }
    }
    /// 将商品类型转换为面向操作人的中文展示名称。返回「实物」「虚拟」「线下服务」或「卡券」。
    pub fn label(self) -> &'static str {
        match self {
            Self::Physical => "实物",
            Self::Virtual => "虚拟",
            Self::OfflineService => "线下服务",
            Self::Voucher => "卡券",
        }
    }
}

/// SKU 与商品之间的目录引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkuFact {
    /// SKU稳定身份。
    pub base: FactIdentity,
    /// 所属商品。
    pub product_id: ProductId,
}

/// 商品稳定类型与当前修订引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductFact {
    /// 商品稳定身份。
    pub base: FactIdentity,
    /// 当前商品修订指针。
    pub stable: CurrentRevisionFact,
    /// 商品稳定类型。
    pub product_kind: ProductKind,
}

/// 商品当前修订的分类引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRevisionFact {
    /// 当前分类。
    pub category_id: ProductCategoryId,
}

/// 责任解析所需的分类父级引用。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductCategoryFact {
    /// 父分类；None 表示根。
    pub parent_category_id: Option<ProductCategoryId>,
}

/// 供给提供方筛选后的 ACTIVE 供给身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferingFact {
    /// 供给身份。
    pub base: FactIdentity,
    /// 当前供给的乐观锁版本。
    pub version: u64,
    /// 当前条款修订指针。
    pub stable: CurrentRevisionFact,
    /// SKU。
    pub sku_id: SkuId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
}

/// 供给当前修订中采购实际消费的条款。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferingRevisionFact {
    /// 条款修订身份。
    pub base: FactIdentity,
    /// 当前条款修订的乐观锁版本。
    pub version: u64,
    /// 提供方条款真实所属供给，不能由消费方当前指针推断。
    pub supplier_offering_id: SupplierOfferingId,
    /// 生效自然日。
    pub valid_from: BusinessDate,
    /// 可选截止自然日。
    pub valid_to: Option<BusinessDate>,
    /// 集采含税价。
    pub bulk_supply_price_gross: Option<UnitPrice>,
    /// 一件代发含税价。
    pub dropship_supply_price_gross: UnitPrice,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 供应商集采起订量。
    pub bulk_minimum_order_quantity: Option<Quantity>,
}

/// 供给可用状态的采购消费值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityStatus {
    /// 提供方明确可供。
    Available,
    /// 提供方任一不可供状态。
    Unavailable,
}

/// 实时供给数量与依据指纹事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityFact {
    /// 可供投影主键与版本。
    pub base: VersionedFactIdentity,
    /// 供给稳定身份。
    pub supplier_offering_id: SupplierOfferingId,
    /// 可供状态。
    pub availability_status: AvailabilityStatus,
    /// None 表示无数量上限。
    pub available_quantity: Option<Quantity>,
    /// 上游来源版本；缺失保持 None。
    pub source_revision_token: Option<String>,
}

/// 供应商角色的当前商务修订引用。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplierRoleFact {
    /// 当前商务修订指针。
    pub current_commercial_profile_revision_id: Option<SupplierCommercialProfileRevisionId>,
}

/// 由供应商领域解释的当前商务条款。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierCommercialFact {
    /// 供应商领域解释后的付款代码，空值保持原缺失语义。
    pub payment_term_code: String,
    /// 供应商领域解释后的经营类目。
    pub business_category: Option<String>,
}

impl SupplierCommercialFact {
    /// 由必填付款代码构造商务事实。返回经营类目为空的商务事实。
    pub fn new(payment_term_code: impl Into<String>) -> Self {
        Self { payment_term_code: payment_term_code.into(), business_category: None }
    }

    /// 设置供应商经营类目。返回更新后的商务事实。
    pub fn with_business_category(mut self, business_category: impl Into<String>) -> Self {
        self.business_category = Some(business_category.into());
        self
    }

    /// 复制供给方已解释的付款条件代码，供采购流程使用；空字符串仍表示缺失。
    pub fn effective_payment_term_code(&self) -> String {
        self.payment_term_code.clone()
    }

    /// 复制供给方提供的经营类目以供采购匹配，未登记时保持 `None`。
    pub fn effective_business_category(&self) -> Option<String> {
        self.business_category.clone()
    }
}

/// 现有库存直接预占对销售覆盖的贡献。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingStockReservationFact {
    /// 稳定销售行。
    pub sales_order_line_id: SalesOrderLineId,
    /// 仍预占数量。
    pub reserved_quantity: Quantity,
    /// 已消耗数量。
    pub consumed_quantity: Quantity,
}

/// 库存选源和依据指纹所需余额事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockBalanceFact {
    /// 余额身份与版本。
    pub base: VersionedFactIdentity,
    /// 仓库身份。
    pub warehouse_id: WarehouseId,
    /// SKU身份。
    pub sku_id: SkuId,
    /// 当前可分配数量。
    pub available_quantity: Quantity,
}

/// 供应商受控付款条件解析后的采购消费事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentTermFact {
    /// 提供方受控先款条件的最低比例。
    pub prepay_minimum_ratio: Option<Rate>,
    /// 提供方规范化的受控代码。
    pub canonical_code: String,
    /// 先款门禁。
    pub prepay_gate: bool,
    /// 以最晚预计交期为基准的天数；None 表示审批日付款。
    pub days_after_delivery: Option<u64>,
    /// 自然周期及期末后付款天数；旧货到规则为空。
    pub calendar_due: Option<(erp_core::common::calendar::CalendarPeriod, u16)>,
}

impl PaymentTermFact {
    /// 由必填规范化代码构造付款事实。返回无先款门禁、无比例、无账期的付款事实。
    pub fn new(canonical_code: impl Into<String>) -> Self {
        Self {
            canonical_code: canonical_code.into(),
            prepay_gate: false,
            prepay_minimum_ratio: None,
            days_after_delivery: None,
            calendar_due: None,
        }
    }

    /// 设置先款门禁。返回更新后的付款事实。
    pub fn with_prepay_gate(mut self, prepay_gate: bool) -> Self {
        self.prepay_gate = prepay_gate;
        self
    }

    /// 设置受控先款最低比例。返回更新后的付款事实。
    pub fn with_prepay_minimum_ratio(mut self, prepay_minimum_ratio: Rate) -> Self {
        self.prepay_minimum_ratio = Some(prepay_minimum_ratio);
        self
    }

    /// 设置预计交期后付款天数。返回更新后的付款事实。
    pub fn with_days_after_delivery(mut self, days_after_delivery: u64) -> Self {
        self.days_after_delivery = Some(days_after_delivery);
        self
    }

    /// 设置自然周期到期规则。返回更新后的付款事实。
    pub fn with_calendar_due(
        mut self,
        calendar_due: (erp_core::common::calendar::CalendarPeriod, u16),
    ) -> Self {
        self.calendar_due = Some(calendar_due);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{ProductCategoryFact, ProductKind, SupplierCommercialFact, SupplierRoleFact};

    // 已冻结的采购责任 selector 必须沿用商品类型 wire 代码。
    #[test]
    fn product_kind_preserves_selector_wire_codes() {
        for (kind, code) in [
            (ProductKind::Physical, "PHYSICAL"),
            (ProductKind::Virtual, "VIRTUAL"),
            (ProductKind::OfflineService, "OFFLINE_SERVICE"),
            (ProductKind::Voucher, "VOUCHER"),
        ] {
            let value = serde_json::Value::String(code.to_string());
            assert_eq!(serde_json::to_value(kind).unwrap(), value);
            assert_eq!(serde_json::from_value::<ProductKind>(value).unwrap(), kind);
            assert_eq!(kind.as_str(), code);
        }
    }

    #[test]
    fn optional_fact_wrappers_default_to_empty() {
        assert!(ProductCategoryFact::default().parent_category_id.is_none());
        assert!(SupplierRoleFact::default().current_commercial_profile_revision_id.is_none());
    }

    #[test]
    fn supplier_commercial_fact_constructor_sets_code_only() {
        let fact = SupplierCommercialFact::new("NET-30");
        assert_eq!(fact.payment_term_code, "NET-30");
        assert!(fact.business_category.is_none());
        let categorized = SupplierCommercialFact::new("NET-30").with_business_category("茶叶");
        assert_eq!(categorized.business_category.as_deref(), Some("茶叶"));
    }
}
