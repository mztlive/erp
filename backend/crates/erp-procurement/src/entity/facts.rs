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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Physical => "PHYSICAL",
            Self::Virtual => "VIRTUAL",
            Self::OfflineService => "OFFLINE_SERVICE",
            Self::Voucher => "VOUCHER",
        }
    }
    /// 返回原业务展示标签。
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductCategoryFact {
    /// 父分类；None 表示根。
    pub parent_category_id: Option<ProductCategoryId>,
}

/// 供给提供方筛选后的 ACTIVE 供给身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferingFact {
    /// 供给身份。
    pub base: FactIdentity,
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
    /// 生效自然日。
    pub valid_from: BusinessDate,
    /// 可选截止自然日。
    pub valid_to: Option<BusinessDate>,
    /// 集采含税价。
    pub bulk_supply_price_gross: UnitPrice,
    /// 一件代发含税价。
    pub dropship_supply_price_gross: UnitPrice,
    /// 进项税率。
    pub input_tax_rate: Rate,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// 返回提供方已经解释的付款条件原值。
    pub fn effective_payment_term_code(&self) -> String {
        self.payment_term_code.clone()
    }
    /// 返回提供方已经解释的经营类目。
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

/// 身份提供方当前账号资格事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityOwnerFact {
    /// 账号身份。
    pub id: String,
    /// 当前姓名。
    pub name: String,
    /// 账号当前是否可登录。
    pub can_login: bool,
    /// 是否后台管理员类型。
    pub is_admin: bool,
}

/// 审计记录中采购幂等回放所需字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditReceiptFact {
    /// 原记录执行结果。
    pub success: bool,
    /// 原操作人。
    pub actor_id: String,
    /// 原动作。
    pub action: String,
    /// 原资源类型。
    pub resource_type: String,
    /// 原目标资源。
    pub resource_id: Option<String>,
    /// 原收据消息。
    pub message: Option<String>,
}

/// 供应商受控付款条件解析后的采购消费事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentTermFact {
    /// 提供方规范化的受控代码。
    pub canonical_code: String,
    /// 先款门禁。
    pub prepay_gate: bool,
    /// 以最晚预计交期为基准的天数；None 表示审批日付款。
    pub days_after_delivery: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::ProductKind;

    /// 已冻结的采购责任 selector 必须沿用商品类型 wire 代码。
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
}
