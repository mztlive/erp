//! 采购创建依据领域值对象与纯规则。
//!
//! 创建依据由销售单当前版本的 `GOODS_SERVICE` 行、当前采购覆盖数量和供应商
//! 当前合格供给共同形成；依据精确到销售当前版本、供应商、采购类型、付款条件与
//! 履约责任。本模块承载拆单维度、稳定身份、产品类型映射、履约选项、成本选择、
//! 最大可创建数量与逐行请求规范化等无 I/O 规则；Repository 只负责批量返回
//! 供给、修订、可供投影与供应商结算事实（[`CreationBasisFacts`]），Service
//! 负责加载事实、校验任务归属与 RBAC、注入业务日期并执行事务。

use std::collections::HashSet;
use std::str::FromStr;

use erp_core::common::time::BusinessDate;
use erp_core::ids::{SupplierAccountId, WarehouseId};
use erp_core::money::{Quantity, Rate, UnitPrice};
use erp_core::{Error, Result};

use super::command_receipt::digest_parts;
use super::coverage::SalesProcurementCoverageLine;
use super::delivery_judgement::OfferingDeliveryFact;
use super::types::{FulfillmentResponsibility, PurchaseType, SupplyPriceBasis};
use crate::entity::facts::{
    AvailabilityFact as SupplierOfferingAvailability, OfferingFact as SupplierOffering,
    OfferingRevisionFact as SupplierOfferingRevision, ProductKind, SalesOrderBasisFact as SalesOrder,
    SalesRevisionFact as SalesOrderRevision, SupplierCommercialFact as SupplierCommercialProfileRevision,
    SupplierRoleFact as SupplierAccount,
};
use crate::entity::purchase_order::{PurchaseDeliveryAllocation, PurchaseInquiryQuoteSource};

/// 单条销售当前版本明细的合格供应商供给。
#[derive(Debug, Clone)]
pub struct LineSupply {
    /// 供给稳定身份。
    pub offering: SupplierOffering,
    /// 当前有效商业条款修订。
    pub revision: SupplierOfferingRevision,
    /// 当前可供投影。
    pub availability: SupplierOfferingAvailability,
    /// 当前生效配送政策。
    pub delivery: OfferingDeliveryFact,
}

/// 一条可进入精确创建依据的销售当前版本行。
#[derive(Debug, Clone)]
pub struct BasisLine {
    /// 服务器核验的专项条款；空值使用通用供给。
    pub inquiry_quote: Option<PurchaseInquiryQuoteSource>,
    /// 当前销售版本行及采购覆盖摘要。
    pub coverage: SalesProcurementCoverageLine,
    /// 本供应商被确定选用的供给。
    pub supply: LineSupply,
    /// 本供应商本次最多可创建数量。
    pub max_create_quantity: Quantity,
}

impl BasisLine {
    /// 读取本依据实际采购成本，专项条款独立于通用价。
    /// # 参数
    /// `basis` 为通用采购价格口径。
    /// # 返回
    /// 返回专项原始采购价或通用供给价。
    /// # 错误
    /// 选择未支持的采购价格口径时拒绝。
    pub fn unit_cost(&self, basis: SupplyPriceBasis) -> Result<UnitPrice> {
        self.inquiry_quote
            .as_ref()
            .map_or_else(|| supply_cost(&self.supply.revision, basis), |quote| Ok(quote.unit_cost_gross))
    }
    /// 读取实际采用条款税率。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回专项税率或通用供给税率。
    /// # 错误
    /// 无。
    pub fn tax_rate(&self) -> Rate {
        self.inquiry_quote
            .as_ref()
            .map(|quote| quote.input_tax_rate)
            .unwrap_or(self.supply.revision.input_tax_rate)
    }
    /// 冻结通用供给身份与独立专项采用链。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回可持久化的实际采用来源。
    /// # 错误
    /// 供给归属或专项身份非法时拒绝。
    pub fn offering_source(&self) -> Result<super::PurchaseOfferingSource> {
        let mut source = super::PurchaseOfferingSource::from_supply(&self.supply)?;
        if let Some(quote) = &self.inquiry_quote {
            quote.validate()?;
            source.inquiry_quote = Some(quote.clone());
        }
        Ok(source)
    }
}

/// 一张采购单的精确拆分维度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasisScope {
    /// 专项报价版本参与拆单；与通用供给保持独立。
    pub inquiry_quote_revision_id: Option<String>,
    /// 唯一供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件。
    pub payment_term_code: String,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 价格依据；代发与集采分别成单。
    pub price_basis: SupplyPriceBasis,
}

/// 一条精确采购创建依据。
#[derive(Debug, Clone)]
pub struct BasisGroup {
    /// 销售当前版本。
    pub revision: SalesOrderRevision,
    /// 精确拆分维度。
    pub scope: BasisScope,
    /// 供应商经营类目（不参与拆单，仅随依据展示）。
    pub business_category: Option<String>,
    /// 可采购明细。
    pub lines: Vec<BasisLine>,
}

/// 已规范化的本次采购行请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestedLine {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 本次采购数量。
    pub quantity: Quantity,
    /// 采购确认的预计交付日。
    pub expected_delivery_date: BusinessDate,
    /// 本次承担的地址与数量。
    pub delivery_allocations: Vec<PurchaseDeliveryAllocation>,
}

/// 采购创建依据计算所需的最小持久化事实集合。
///
/// Repository 一次批量返回任务涉及 SKU 的 ACTIVE 供给、当前修订、可供投影、
/// 供应商角色、当前商务资料与法定名称；本结构不承载任何计算规则，合格性筛选
/// 与拆单分组由 Service 在事务内基于本事实解释完成。
#[derive(Debug, Clone, Default)]
pub struct CreationBasisFacts {
    /// 正式销售存在冻结询价来源时，通用候选须显式改选。
    pub source_inquiry_id: Option<String>,
    /// ACTIVE 且未删除的供给；Repository 已按 SKU、供应商与供给 ID 稳定排序。
    pub offerings: Vec<SupplierOffering>,
    /// 供给当前商业条款修订，按修订主键。
    pub revisions: std::collections::HashMap<String, SupplierOfferingRevision>,
    /// 供给实时可供投影，按供给主键。
    pub availabilities: std::collections::HashMap<String, SupplierOfferingAvailability>,
    /// 供应商角色，按供应商主键。
    pub suppliers: std::collections::HashMap<String, SupplierAccount>,
    /// 供应商当前商务资料修订，按修订主键。
    pub commercial_profiles: std::collections::HashMap<String, SupplierCommercialProfileRevision>,
    /// 供应商当前法定名称，按供应商主键；关联缺失时不包含该键。
    pub supplier_names: std::collections::HashMap<String, String>,
    /// 供给当前生效配送政策，按供给主键；缺失时按全国配送、包邮。
    pub deliveries: std::collections::HashMap<String, OfferingDeliveryFact>,
}

impl RequestedLine {
    /// 由原始请求文本规范化并校验一条本次采购行。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 稳定销售行原始文本
    /// * `quantity` - 本次采购数量原始文本
    /// * `expected_delivery_date` - 预计交付日原始文本
    ///
    /// # 返回
    /// 返回去除首尾空白且数量已类型化的请求行。
    ///
    /// # 错误
    /// 数量或预计交付日非法、数量不大于零时返回领域错误。
    ///
    /// # 关键业务约束
    /// 不做重复检查与排序，集合级规则由 [`normalize_requested_lines`] 承担。
    pub fn parse(sales_order_line_id: &str, quantity: &str, expected_delivery_date: &str) -> Result<Self> {
        let sales_order_line_id = sales_order_line_id.trim().to_string();
        let quantity = Quantity::from_str(quantity.trim())
            .map_err(|error| Error::from(format!("本次数量非法: {error}")))?;
        let expected_delivery_date = BusinessDate::from_str(expected_delivery_date.trim())
            .map_err(|error| Error::from(format!("预计交付日非法: {error}")))?;
        if quantity <= zero_quantity() {
            return Err(Error::from("本次数量必须大于 0"));
        }
        Ok(Self { sales_order_line_id, quantity, expected_delivery_date, delivery_allocations: Vec::new() })
    }
}

/// 规范化并校验逐行本次采购数量集合。
///
/// # 参数
/// * `lines` - 已逐行解析的请求行
///
/// # 返回
/// 返回稳定行不重复且按稳定行升序排列的请求行。
///
/// # 错误
/// 同一稳定销售行出现多次时返回领域错误。
///
/// # 关键业务约束
/// 同一稳定销售行在一次命令中只能出现一次。
pub fn normalize_requested_lines(lines: &[RequestedLine]) -> Result<Vec<RequestedLine>> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(lines.len());
    for line in lines {
        if !seen.insert(line.sales_order_line_id.as_str()) {
            return Err(Error::from("本次采购明细包含重复销售行"));
        }
        normalized.push(line.clone());
    }
    normalized.sort_by(|left, right| left.sales_order_line_id.cmp(&right.sales_order_line_id));
    Ok(normalized)
}

/// 形成包含 guard、当前版本、精确范围和逐行供给事实的依据 ID。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `group` - 精确依据分组
/// * `work_item_id` - 冻结本依据责任范围的开放任务
/// * `target_warehouse_id` - 本张采购计划选择的目标仓库；可选择依据阶段为空
///
/// # 返回
/// 返回 `{sales_order_id}:{sha256}` 稳定依据 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// guard 每次成功创建后推进，使作废释放的剩余量可形成新依据；同一范围拆向不同
/// 目标仓时身份不同。
pub fn basis_id_for(
    order: &SalesOrder,
    group: &BasisGroup,
    work_item_id: &str,
    target_warehouse_id: Option<&WarehouseId>,
) -> String {
    let mut parts = vec![
        order.base.id.clone(),
        work_item_id.to_string(),
        order.procurement_guard_version.to_string(),
        group.revision.base.id.clone(),
        basis_scope_key(&group.scope),
    ];
    parts.extend(group.lines.iter().map(basis_line_fingerprint));
    compose_basis_id(&order.base.id, parts, target_warehouse_id)
}

/// 以规范化依据片段和可选目标仓形成固定长度创建依据 ID。
///
/// 目标仓只在已经完成仓库选择的采购计划中加入；可选择依据保持原有无仓库身份。
///
/// # 参数
/// * `sales_order_id` - 销售单稳定身份，作为 ID 前缀。
/// * `parts` - 已规范化的依据片段；有目标仓时在参与摘要前追加仓库片段。
/// * `target_warehouse_id` - 已选定的目标收货仓；`None` 保持无仓库身份。
///
/// # 返回
/// 返回 `{sales_order_id}:{sha256}`。摘要按 `parts` 的传入顺序计算。
///
/// # 错误
/// 不返回错误。
pub fn compose_basis_id(
    sales_order_id: &str,
    mut parts: Vec<String>,
    target_warehouse_id: Option<&WarehouseId>,
) -> String {
    if let Some(target_warehouse_id) = target_warehouse_id {
        parts.push(format!("target_warehouse:{target_warehouse_id}"));
    }
    format!("{sales_order_id}:{}", digest_parts(parts))
}

/// 形成单条依据行的供给与剩余量指纹。
///
/// # 参数
/// * `line` - 精确依据行
///
/// # 返回
/// 返回稳定行、当前版本行、数量与供给版本组成的规范化字符串。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 可供投影版本变化会使旧依据失效。
fn basis_line_fingerprint(line: &BasisLine) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        stable_line_id(line),
        line.coverage.revision_line.base.id,
        line.coverage.summary.remaining_quantity,
        line.max_create_quantity,
        line.supply.offering.base.id,
        line.supply.offering.version,
        line.supply.revision.base.id,
        line.supply.revision.version,
        line.supply.availability.base.id,
        line.supply.availability.base.version,
        line.supply.availability.source_revision_token.as_deref().unwrap_or("-"),
        line.inquiry_quote
            .as_ref()
            .map(|quote| format!("{}:{}", quote.quote_revision_id, quote.quote_version))
            .unwrap_or_default(),
    )
}

/// 形成精确拆分范围规范化键。
///
/// # 参数
/// * `scope` - 精确拆分范围
///
/// # 返回
/// 返回供应商、采购类型、付款条件、履约责任和价格依据拼接键。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 该键只用于分组和指纹，不作为数据库自然键。
pub fn basis_scope_key(scope: &BasisScope) -> String {
    format!(
        "{}|{}|{}|{}|{}{}",
        scope.supplier_id,
        scope.purchase_type.as_str(),
        scope.payment_term_code,
        scope.fulfillment_responsibility.as_str(),
        scope.price_basis.as_str(),
        scope.inquiry_quote_revision_id.as_ref().map(|id| format!("|INQUIRY:{id}")).unwrap_or_default(),
    )
}

/// 返回依据行的稳定销售行 ID。
///
/// # 参数
/// * `line` - 精确依据行
///
/// # 返回
/// 返回跨销售版本稳定的销售行 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 所有覆盖与请求匹配均使用稳定销售行，不按 SKU 猜测。
pub fn stable_line_id(line: &BasisLine) -> &str {
    line.coverage.revision_line.sales_order_line_id.as_ref()
}

/// 计算供应商本次最大可创建数量。
///
/// # 参数
/// * `remaining` - 销售当前版本剩余量
/// * `available` - 供应商当前可供上限；空表示无限制
///
/// # 返回
/// 返回 `min(remaining, available)` 或无上限时的 `remaining`。
///
/// # 错误
/// 可供数量为负时返回一致性错误。
///
/// # 关键业务约束
/// 供应不足允许形成部分数量依据。
pub fn maximum_create_quantity(remaining: Quantity, available: Option<Quantity>) -> Result<Quantity> {
    let Some(available) = available else {
        return Ok(remaining);
    };
    if available < zero_quantity() {
        return Err(Error::from("供应商可供数量不能为负"));
    }
    Ok(remaining.min(available))
}

/// 取供给含税成本。
///
/// # 参数
/// * `revision` - 当前有效供给条款
/// * `price_basis` - 本采购依据选择的价格依据
///
/// # 返回
/// 集采返回已维护的集采价，代发返回一件代发价。
///
/// # 错误
/// 供给未提供集采价格时拒绝集采取价。
///
/// # 关键业务约束
/// 成本只从依据确定的当前供给修订读取；缺失集采条款不回退到代发价。
pub fn supply_cost(revision: &SupplierOfferingRevision, price_basis: SupplyPriceBasis) -> Result<UnitPrice> {
    match price_basis {
        SupplyPriceBasis::Bulk => revision
            .bulk_supply_price_gross
            .ok_or_else(|| Error::from("该供给未提供集采条款，不能按集采采购")),
        SupplyPriceBasis::Dropship => Ok(revision.dropship_supply_price_gross),
    }
}

/// 由商品稳定业务类型确定采购类型。
///
/// # 参数
/// * `kind` - 商品稳定业务类型
///
/// # 返回
/// 返回采购类型；卡券不得进入商品/服务采购路径。
///
/// # 错误
/// 卡券进入商品/服务采购路径时返回业务错误。
///
/// # 关键业务约束
/// 销售不得提交或覆盖采购类型。
pub fn purchase_type_from_product_kind(kind: ProductKind) -> Result<PurchaseType> {
    match kind {
        ProductKind::Physical => Ok(PurchaseType::Physical),
        ProductKind::Virtual => Ok(PurchaseType::Virtual),
        ProductKind::OfflineService => Ok(PurchaseType::Service),
        ProductKind::Voucher => Err(Error::from("卡券商品不能进入商品/服务采购建单路径")),
    }
}

/// 返回商品类型允许采购选择的履约责任与价格依据组合。
///
/// # 参数
/// * `kind` - 商品稳定业务类型
///
/// # 返回
/// 返回稳定顺序的采购方案集合。
///
/// # 错误
/// 卡券进入商品/服务采购路径时返回业务错误。
///
/// # 关键业务约束
/// 实物形成集采入仓、代发直发和集采直发三种方案；虚拟商品与线下服务只按
/// 一件代发价由商品事实唯一限定履约责任。
pub fn purchase_options(
    kind: ProductKind,
) -> Result<&'static [(FulfillmentResponsibility, SupplyPriceBasis)]> {
    match kind {
        ProductKind::Physical => Ok(&[
            (FulfillmentResponsibility::Warehouse, SupplyPriceBasis::Bulk),
            (FulfillmentResponsibility::SupplierDirect, SupplyPriceBasis::Dropship),
            (FulfillmentResponsibility::SupplierDirect, SupplyPriceBasis::Bulk),
        ]),
        ProductKind::Virtual => Ok(&[(FulfillmentResponsibility::Electronic, SupplyPriceBasis::Dropship)]),
        ProductKind::OfflineService => {
            Ok(&[(FulfillmentResponsibility::Service, SupplyPriceBasis::Dropship)])
        },
        ProductKind::Voucher => Err(Error::from("卡券商品不能进入商品/服务采购建单路径")),
    }
}

/// 返回合法采购数量零值。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回六位精度数量零值。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 只用于边界比较，不代表缺失业务数量。
///
/// # Panics
/// 数量文本 `"0"` 无法解析时 panic；零数量是合法数量，panic 只表示数量类型不变量被破坏。
fn zero_quantity() -> Quantity {
    Quantity::from_str("0").expect("零数量合法")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        SalesOrderRevisionLineId, SkuId, SupplierAccountId, SupplierOfferingId, WarehouseId,
    };
    use erp_core::money::{Quantity, Rate, UnitPrice};

    use super::{
        BasisGroup, BasisLine, BasisScope, LineSupply, RequestedLine, basis_id_for, basis_scope_key,
        compose_basis_id, maximum_create_quantity, normalize_requested_lines, purchase_options,
        purchase_type_from_product_kind, supply_cost,
    };
    use crate::entity::facts::{
        AvailabilityFact as SupplierOfferingAvailability, AvailabilityStatus, CurrentRevisionFact,
        FactIdentity, OfferingFact as SupplierOffering, OfferingRevisionFact as SupplierOfferingRevision,
        ProductKind, SalesCustomerSnapshotFact, SalesGoodsLineFact as SalesOrderGoodsServiceLineRevision,
        SalesLineType as LineType, SalesOrderBasisFact, SalesRevisionFact as SalesOrderRevision,
        SalesRevisionLineFact as SalesOrderRevisionLine, VersionedFactIdentity,
    };
    use crate::entity::purchase_order::coverage::SalesProcurementCoverageLine;
    use crate::entity::purchase_order::{
        FulfillmentResponsibility, ProcurementCoverageSummary, PurchaseType, SupplyPriceBasis,
    };

    /// 构造销售当前版本头。
    fn revision(id: &str) -> SalesOrderRevision {
        SalesOrderRevision {
            base: FactIdentity { id: format!("rev-{id}") },
            customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".to_string() },
            contract_snapshot: None,
        }
    }

    /// 构造销售当前版本公共行。
    fn revision_line(id: &str, stable_line_id: &str) -> SalesOrderRevisionLine {
        SalesOrderRevisionLine {
            base: FactIdentity { id: id.to_string() },
            sales_order_line_id: erp_core::ids::SalesOrderLineId::new(stable_line_id),
            line_no: 1,
            line_type: LineType::GoodsService,
            item_name_snapshot: "商品".to_string(),
            spec_snapshot: Some("规格".to_string()),
            unit_snapshot: Some("件".to_string()),
        }
    }

    /// 构造销售当前版本商品/服务子类型行。
    fn goods_line(revision_line_id: &str) -> SalesOrderGoodsServiceLineRevision {
        SalesOrderGoodsServiceLineRevision {
            assigned_delivery_allocations: Vec::new(),
            delivery_allocations: Vec::new(),

            revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: erp_core::ids::SkuRevisionId::new("skur-1"),
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: Quantity::from_str("10").unwrap(),
            base_unit_code: "件".to_string(),
        }
    }

    /// 构造一条销售覆盖目标行。
    fn coverage_line(stable_line_id: &str, total: &str, covered: &str) -> SalesProcurementCoverageLine {
        SalesProcurementCoverageLine {
            quantity_scale: Some(6),
            revision_line: revision_line("sorl-1", stable_line_id),
            goods_line: goods_line("sorl-1"),
            product_kind: ProductKind::Physical,
            summary: ProcurementCoverageSummary::new(
                Quantity::from_str(total).unwrap(),
                Quantity::from_str(covered).unwrap(),
            )
            .unwrap(),
        }
    }

    /// 构造供给稳定身份。
    fn offering(id: &str, supplier_id: &str) -> SupplierOffering {
        SupplierOffering {
            version: 1,
            base: FactIdentity { id: id.to_string() },
            stable: CurrentRevisionFact::default(),
            sku_id: SkuId::new("sku-1"),
            supplier_id: SupplierAccountId::new(supplier_id),
        }
    }

    /// 构造供给商业条款修订。
    fn offering_revision(
        offering_id: &str,
        valid_from: &str,
        valid_to: Option<&str>,
    ) -> SupplierOfferingRevision {
        SupplierOfferingRevision {
            version: 1,
            base: FactIdentity { id: format!("offrev-{offering_id}") },
            supplier_offering_id: SupplierOfferingId::new(offering_id),
            dropship_supply_price_gross: UnitPrice::from_str("6").unwrap(),
            bulk_supply_price_gross: Some(UnitPrice::from_str("5").unwrap()),
            input_tax_rate: Rate::from_str("0.13").unwrap(),
            bulk_minimum_order_quantity: Some(Quantity::from_str("1").unwrap()),
            valid_from: erp_core::common::time::BusinessDate::from_str(valid_from).unwrap(),
            valid_to: valid_to.map(|value| erp_core::common::time::BusinessDate::from_str(value).unwrap()),
        }
    }

    /// 构造供给实时可供投影。
    fn availability(offering_id: &str, quantity: Option<&str>) -> SupplierOfferingAvailability {
        SupplierOfferingAvailability {
            base: VersionedFactIdentity { id: format!("avail-{offering_id}"), version: 1 },
            supplier_offering_id: SupplierOfferingId::new(offering_id),
            availability_status: AvailabilityStatus::Available,
            available_quantity: quantity.map(|value| Quantity::from_str(value).unwrap()),
            source_revision_token: None,
        }
    }

    /// 构造一条合格供给。
    fn line_supply(offering_id: &str, supplier_id: &str) -> LineSupply {
        LineSupply {
            offering: offering(offering_id, supplier_id),
            revision: offering_revision(offering_id, "2026-01-01", None),
            availability: availability(offering_id, Some("8")),
            delivery: Default::default(),
        }
    }

    /// 构造销售稳定单。
    fn sales_order(id: &str) -> SalesOrderBasisFact {
        SalesOrderBasisFact {
            base: FactIdentity { id: id.to_string() },
            order_no: format!("SO-{id}"),
            procurement_guard_version: 3,
            is_effective: true,
        }
    }

    /// 构造一条完整精确依据分组。
    fn basis_group(supplier_id: &str, payment_term_code: &str) -> BasisGroup {
        let supply = line_supply("offering-1", supplier_id);
        let line = BasisLine {
            inquiry_quote: None,
            coverage: coverage_line("sol-1", "10", "2"),
            supply,
            max_create_quantity: Quantity::from_str("8").unwrap(),
        };
        BasisGroup {
            revision: revision("1"),
            scope: BasisScope {
                inquiry_quote_revision_id: None,
                supplier_id: SupplierAccountId::new(supplier_id),
                purchase_type: PurchaseType::Physical,
                payment_term_code: payment_term_code.to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                price_basis: SupplyPriceBasis::Bulk,
            },
            business_category: None,
            lines: vec![line],
        }
    }

    /// 相同输入重复构造依据 ID 完全一致。
    #[test]
    fn basis_id_is_deterministic_for_identical_inputs() {
        let order = sales_order("so-1");
        let group = basis_group("sup-1", "NET-30");

        let first = basis_id_for(&order, &group, "wi-1", None);
        let second = basis_id_for(&order, &group, "wi-1", None);

        assert_eq!(first, second);
        assert_eq!(first.split(':').count(), 2);
        assert_eq!(first.split(':').next().unwrap(), "so-1");
        assert!(first.split(':').nth(1).unwrap().len() == 64);
    }

    #[test]
    fn basis_identity_changes_when_selected_offering_or_terms_version_changes() {
        let order = sales_order("sales");
        let mut group = basis_group("supplier", "NET-30");
        let original = basis_id_for(&order, &group, "task", None);
        group.lines[0].supply.offering.version += 1;
        let offering_changed = basis_id_for(&order, &group, "task", None);
        assert_ne!(original, offering_changed);
        group.lines[0].supply.revision.version += 1;
        assert_ne!(offering_changed, basis_id_for(&order, &group, "task", None));
    }

    /// 同一范围拆向不同目标仓时必须形成不同的数据库唯一身份。
    #[test]
    fn target_warehouse_scopes_creation_basis_identity() {
        let common_parts = vec!["guard-1".to_string(), "scope-1".to_string()];
        let first = compose_basis_id("so-1", common_parts.clone(), Some(&WarehouseId::new("warehouse-1")));
        let second = compose_basis_id("so-1", common_parts.clone(), Some(&WarehouseId::new("warehouse-2")));
        let selectable = compose_basis_id("so-1", common_parts, None);

        assert_ne!(first, second);
        assert_ne!(first, selectable);
        assert_ne!(second, selectable);
    }

    /// 精确范围同时区分供应商、类型、付款条件和履约责任。
    #[test]
    fn basis_scope_key_contains_exact_split_dimensions() {
        let scope = BasisScope {
            inquiry_quote_revision_id: None,
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            price_basis: SupplyPriceBasis::Bulk,
        };

        assert_eq!(basis_scope_key(&scope), "sup-1|PHYSICAL|NET-30|WAREHOUSE|BULK");
    }

    /// 不同供应商、付款条件或履约责任必须形成不同依据 ID。
    #[test]
    fn basis_id_changes_with_split_dimensions() {
        let order = sales_order("so-1");
        let base = basis_group("sup-1", "NET-30");
        let base_id = basis_id_for(&order, &base, "wi-1", None);

        let mut other_supplier = base.clone();
        other_supplier.scope.supplier_id = SupplierAccountId::new("sup-2");
        assert_ne!(base_id, basis_id_for(&order, &other_supplier, "wi-1", None));

        let mut other_term = base.clone();
        other_term.scope.payment_term_code = "NET-60".to_string();
        assert_ne!(base_id, basis_id_for(&order, &other_term, "wi-1", None));

        let mut other_basis = base.clone();
        other_basis.scope.price_basis = SupplyPriceBasis::Dropship;
        assert_ne!(base_id, basis_id_for(&order, &other_basis, "wi-1", None));

        let mut other_responsibility = base.clone();
        other_responsibility.scope.fulfillment_responsibility = FulfillmentResponsibility::SupplierDirect;
        assert_ne!(base_id, basis_id_for(&order, &other_responsibility, "wi-1", None));

        assert_ne!(base_id, basis_id_for(&order, &base, "wi-2", None));
    }

    /// 商品稳定类型决定采购类型，销售字段不得参与。
    #[test]
    fn product_kind_determines_purchase_type() {
        assert_eq!(purchase_type_from_product_kind(ProductKind::Physical).unwrap(), PurchaseType::Physical);
        assert_eq!(purchase_type_from_product_kind(ProductKind::Virtual).unwrap(), PurchaseType::Virtual);
        assert_eq!(
            purchase_type_from_product_kind(ProductKind::OfflineService).unwrap(),
            PurchaseType::Service
        );
        assert!(purchase_type_from_product_kind(ProductKind::Voucher).is_err());
    }

    /// 实物形成集采入仓、代发直发和集采直发，其他商品类型只允许其固有路线。
    #[test]
    fn product_kind_limits_purchase_options() {
        assert_eq!(
            purchase_options(ProductKind::Physical).unwrap(),
            &[
                (FulfillmentResponsibility::Warehouse, SupplyPriceBasis::Bulk),
                (FulfillmentResponsibility::SupplierDirect, SupplyPriceBasis::Dropship),
                (FulfillmentResponsibility::SupplierDirect, SupplyPriceBasis::Bulk),
            ]
        );
        assert_eq!(
            purchase_options(ProductKind::Virtual).unwrap(),
            &[(FulfillmentResponsibility::Electronic, SupplyPriceBasis::Dropship)]
        );
        assert_eq!(
            purchase_options(ProductKind::OfflineService).unwrap(),
            &[(FulfillmentResponsibility::Service, SupplyPriceBasis::Dropship)]
        );
        assert!(purchase_options(ProductKind::Voucher).is_err());
    }

    /// 有限可供量不足销售剩余时允许形成部分数量。
    #[test]
    fn limited_availability_uses_partial_quantity() {
        let remaining = Quantity::from_str("10").unwrap();
        let available = Quantity::from_str("3.5").unwrap();

        assert_eq!(maximum_create_quantity(remaining, Some(available)).unwrap(), available);
        assert_eq!(maximum_create_quantity(remaining, None).unwrap(), remaining);
        assert!(maximum_create_quantity(remaining, Some(Quantity::from_str("-1").unwrap())).is_err());
    }

    /// 集采取集采价，代发取一件代发价，与履约责任无关。
    #[test]
    fn supply_cost_follows_price_basis() {
        let revision = offering_revision("offering-1", "2026-01-01", None);

        assert_eq!(
            supply_cost(&revision, SupplyPriceBasis::Bulk).unwrap(),
            UnitPrice::from_str("5").unwrap()
        );
        assert_eq!(
            supply_cost(&revision, SupplyPriceBasis::Dropship).unwrap(),
            UnitPrice::from_str("6").unwrap()
        );
    }

    #[test]
    fn dropship_only_supply_cannot_be_bought_as_bulk() {
        let mut revision = offering_revision("api", "2026-01-01", None);
        revision.bulk_supply_price_gross = None;
        revision.bulk_minimum_order_quantity = None;
        assert_eq!(supply_cost(&revision, SupplyPriceBasis::Dropship).unwrap(), "6".parse().unwrap());
        assert!(supply_cost(&revision, SupplyPriceBasis::Bulk).is_err());
    }

    /// 请求行解析去除空白并校验数量与交付日。
    #[test]
    fn requested_line_parse_normalizes_and_validates() {
        let line = RequestedLine::parse(" sol-1 ", " 2.5 ", " 2026-08-25 ").unwrap();

        assert_eq!(line.sales_order_line_id, "sol-1");
        assert_eq!(line.quantity, Quantity::from_str("2.5").unwrap());
        assert_eq!(
            line.expected_delivery_date,
            erp_core::common::time::BusinessDate::from_str("2026-08-25").unwrap()
        );
        assert!(RequestedLine::parse("sol-1", "abc", "2026-08-25").is_err());
        assert!(RequestedLine::parse("sol-1", "1", "not-a-date").is_err());
        assert!(RequestedLine::parse("sol-1", "0", "2026-08-25").is_err());
        assert!(RequestedLine::parse("sol-1", "-1", "2026-08-25").is_err());
    }

    /// 集合规范化拒绝重复稳定行并按稳定行升序稳定排序。
    #[test]
    fn normalize_requested_lines_dedupes_and_sorts() {
        let b = RequestedLine::parse("sol-b", "1", "2026-08-25").unwrap();
        let a = RequestedLine::parse("sol-a", "2", "2026-08-26").unwrap();
        let duplicate = RequestedLine::parse("sol-b", "3", "2026-08-27").unwrap();

        let normalized = normalize_requested_lines(&[b.clone(), a]).unwrap();
        assert_eq!(
            normalized.iter().map(|line| line.sales_order_line_id.clone()).collect::<Vec<_>>(),
            vec!["sol-a".to_string(), "sol-b".to_string()]
        );
        assert!(normalize_requested_lines(&[b, duplicate]).is_err());
        assert_eq!(normalize_requested_lines(&[]).unwrap(), Vec::<RequestedLine>::new());
    }

    /// 依据 ID 只接受销售单加 SHA-256 指纹形态。
    #[test]
    fn basis_id_shape_is_sales_order_plus_sha256() {
        let order = sales_order("so-1");
        let group = basis_group("sup-1", "NET-30");
        let id = basis_id_for(&order, &group, "wi-1", None);

        let parts = id.split(':').collect::<Vec<_>>();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "so-1");
        assert_eq!(parts[1].len(), 64);
        assert!(parts[1].bytes().all(|value| value.is_ascii_hexdigit()));
    }
    fn special_source() -> super::PurchaseInquiryQuoteSource {
        super::PurchaseInquiryQuoteSource {
            inquiry_id: "inquiry".into(),
            requirement_version: 1,
            feedback_round: 2,
            inquiry_line_id: "demand".into(),
            quote_id: "quote".into(),
            quote_revision_id: "quote-v1".into(),
            quote_revision_no: 1,
            quote_version: 1,
            unit_cost_gross: "8.80".parse().unwrap(),
            input_tax_rate: "0.06".parse().unwrap(),
            available_quantity: "20".parse().unwrap(),
            address_limits: vec![],
            expected_delivery_date: "2026-10-22".parse().unwrap(),
            valid_to: Some("2026-11-01".parse().unwrap()),
        }
    }

    #[test]
    fn special_cost_tax_and_source_are_frozen_separately_from_general_offering() {
        let mut group = basis_group("supplier", "NET-30");
        let general_price = group.lines[0].supply.revision.bulk_supply_price_gross;
        group.lines[0].supply.offering.stable.current_revision_id =
            Some(group.lines[0].supply.revision.base.id.clone());
        let source = special_source();
        group.lines[0].inquiry_quote = Some(source.clone());
        assert_eq!(group.lines[0].unit_cost(SupplyPriceBasis::Bulk).unwrap(), source.unit_cost_gross);
        assert_eq!(group.lines[0].tax_rate(), source.input_tax_rate);
        let frozen = group.lines[0].offering_source().unwrap();
        assert_eq!(frozen.inquiry_quote, Some(source));
        assert_eq!(group.lines[0].supply.revision.bulk_supply_price_gross, general_price);
        assert!(
            frozen
                .ensure_current_supply(
                    &group.lines[0].supply,
                    &SupplierAccountId::new("supplier"),
                    &SkuId::new("sku-1"),
                    "1".parse().unwrap(),
                    "2026-10-23".parse().unwrap()
                )
                .is_err()
        );
    }

    #[test]
    fn special_quote_revision_is_an_exact_split_and_basis_identity_dimension() {
        let order = sales_order("sales");
        let mut group = basis_group("supplier", "NET-30");
        let general = basis_id_for(&order, &group, "task", None);
        group.lines[0].inquiry_quote = Some(special_source());
        group.scope.inquiry_quote_revision_id = Some("quote-v1".into());
        assert_ne!(general, basis_id_for(&order, &group, "task", None));
        assert!(basis_scope_key(&group.scope).ends_with("|INQUIRY:quote-v1"));
        let first = basis_id_for(&order, &group, "task", None);
        group.lines[0].inquiry_quote.as_mut().unwrap().quote_revision_id = "quote-v2".into();
        group.scope.inquiry_quote_revision_id = Some("quote-v2".into());
        assert_ne!(first, basis_id_for(&order, &group, "task", None));
    }
    #[test]
    fn special_price_and_tax_cannot_be_overridden_in_any_purchase_line_constructor() {
        let mut group = basis_group("supplier", "NET-30");
        group.lines[0].supply.offering.stable.current_revision_id =
            Some(group.lines[0].supply.revision.base.id.clone());
        let quote = special_source();
        group.lines[0].inquiry_quote = Some(quote.clone());
        let source = group.lines[0].offering_source().unwrap();
        super::super::offering_source::ensure_offering_source(
            super::super::PurchaseLineType::ItemService,
            Some(&source),
            Some(quote.unit_cost_gross),
            Some(quote.input_tax_rate),
        )
        .unwrap();
        assert!(
            super::super::offering_source::ensure_offering_source(
                super::super::PurchaseLineType::ItemService,
                Some(&source),
                Some("1".parse().unwrap()),
                Some(quote.input_tax_rate)
            )
            .is_err()
        );
        assert!(
            super::super::offering_source::ensure_offering_source(
                super::super::PurchaseLineType::ItemService,
                Some(&source),
                Some(quote.unit_cost_gross),
                Some("0".parse().unwrap())
            )
            .is_err()
        );
    }
}
