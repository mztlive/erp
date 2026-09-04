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

use crate::catalog::ProductKind;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{SupplierAccountId, WarehouseId};
use crate::money::{Quantity, UnitPrice};
use crate::sales_order::{SalesOrder, SalesOrderRevision};
use crate::supplier::{SupplierAccount, SupplierCommercialProfileRevision};
use crate::supplier_offering::{SupplierOffering, SupplierOfferingAvailability, SupplierOfferingRevision};

use super::command_receipt::digest_parts;
use super::coverage::SalesProcurementCoverageLine;
use super::types::{FulfillmentResponsibility, PurchaseType};

/// 单条销售当前版本明细的合格供应商供给。
#[derive(Debug, Clone)]
pub struct LineSupply {
    /// 供给稳定身份。
    pub offering: SupplierOffering,
    /// 当前有效商业条款修订。
    pub revision: SupplierOfferingRevision,
    /// 当前可供投影。
    pub availability: SupplierOfferingAvailability,
}

/// 一条可进入精确创建依据的销售当前版本行。
#[derive(Debug, Clone)]
pub struct BasisLine {
    /// 当前销售版本行及采购覆盖摘要。
    pub coverage: SalesProcurementCoverageLine,
    /// 本供应商被确定选用的供给。
    pub supply: LineSupply,
    /// 本供应商本次最多可创建数量。
    pub max_create_quantity: Quantity,
}

/// 一张采购单的精确拆分维度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasisScope {
    /// 唯一供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件。
    pub payment_term_code: String,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
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
}

/// 采购创建依据计算所需的最小持久化事实集合。
///
/// Repository 一次批量返回任务涉及 SKU 的 ACTIVE 供给、当前修订、可供投影、
/// 供应商角色、当前商务资料与法定名称；本结构不承载任何计算规则，合格性筛选
/// 与拆单分组由 Service 在事务内基于本事实解释完成。
#[derive(Debug, Clone, Default)]
pub struct CreationBasisFacts {
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
        Ok(Self {
            sales_order_line_id,
            quantity,
            expected_delivery_date,
        })
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
        if !seen.insert(line.sales_order_line_id.clone()) {
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
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        stable_line_id(line),
        line.coverage.revision_line.base.id,
        line.coverage.summary.remaining_quantity,
        line.max_create_quantity,
        line.supply.offering.base.id,
        line.supply.revision.base.id,
        line.supply.availability.base.id,
        line.supply.availability.base.version,
        line.supply
            .availability
            .source_revision_token
            .as_deref()
            .unwrap_or("-"),
    )
}

/// 形成精确拆分范围规范化键。
///
/// # 参数
/// * `scope` - 精确拆分范围
///
/// # 返回
/// 返回供应商、采购类型、付款条件和履约责任拼接键。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 该键只用于分组和指纹，不作为数据库自然键。
pub fn basis_scope_key(scope: &BasisScope) -> String {
    format!(
        "{}|{}|{}|{}",
        scope.supplier_id,
        scope.purchase_type.as_str(),
        scope.payment_term_code,
        scope.fulfillment_responsibility.as_str(),
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
/// * `responsibility` - 本采购依据选择的履约责任
///
/// # 返回
/// 入仓返回集采价，其他方式返回一件代发价。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 成本只从依据确定的当前供给修订读取。
pub fn supply_cost(
    revision: &SupplierOfferingRevision,
    responsibility: FulfillmentResponsibility,
) -> UnitPrice {
    match responsibility {
        FulfillmentResponsibility::Warehouse => revision.bulk_supply_price_gross,
        FulfillmentResponsibility::SupplierDirect
        | FulfillmentResponsibility::Electronic
        | FulfillmentResponsibility::Service => revision.dropship_supply_price_gross,
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

/// 返回商品类型允许采购选择的履约责任。
///
/// # 参数
/// * `kind` - 商品稳定业务类型
///
/// # 返回
/// 返回稳定顺序的履约责任集合。
///
/// # 错误
/// 卡券进入商品/服务采购路径时返回业务错误。
///
/// # 关键业务约束
/// 实物允许采购在入仓与供应商直发之间选择；其他类型由商品事实唯一限定。
pub fn fulfillment_options(kind: ProductKind) -> Result<&'static [FulfillmentResponsibility]> {
    match kind {
        ProductKind::Physical => Ok(&[
            FulfillmentResponsibility::Warehouse,
            FulfillmentResponsibility::SupplierDirect,
        ]),
        ProductKind::Virtual => Ok(&[FulfillmentResponsibility::Electronic]),
        ProductKind::OfflineService => Ok(&[FulfillmentResponsibility::Service]),
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
fn zero_quantity() -> Quantity {
    Quantity::from_str("0").expect("零数量合法")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::common::time::Instant;
    use crate::ids::{
        SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId, SupplierAccountId,
        SupplierOfferingAvailabilityId, SupplierOfferingId, SupplierOfferingRevisionId, WarehouseId,
    };
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use crate::sales_order::revision::{
        SalesOrderGoodsServiceLineRevision, SalesOrderGoodsServiceLineRevisionData, SalesOrderRevision,
        SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    };
    use crate::sales_order::snapshot::HeaderSnapshotData;
    use crate::sales_order::{CommercialStatus, LineType, ProcurementCoverageSummary, RevisionSource};
    use crate::supplier_offering::{
        AvailabilityStatus, OfferingSourceType, PrefillSourceRefs, SupplierOffering,
        SupplierOfferingAvailability, SupplierOfferingAvailabilityData, SupplierOfferingData,
        SupplierOfferingRevision, SupplierOfferingRevisionData,
    };

    use super::{
        basis_id_for, basis_scope_key, compose_basis_id, fulfillment_options, maximum_create_quantity,
        normalize_requested_lines, purchase_type_from_product_kind, supply_cost, BasisGroup, BasisLine,
        BasisScope, LineSupply, RequestedLine,
    };
    use crate::purchase_order::coverage::SalesProcurementCoverageLine;
    use crate::purchase_order::{FulfillmentResponsibility, PurchaseType};

    /// 构造销售当前版本头。
    fn revision(id: &str) -> SalesOrderRevision {
        SalesOrderRevision::new(
            SalesOrderRevisionId::new(format!("rev-{id}")),
            SalesOrderRevisionData {
                sales_order_id: SalesOrderId::new("so-1"),
                revision_no: 1,
                revision_source: RevisionSource::ErpApproval,
                previous_revision_id: None,
                content_hash: format!("hash-{id}"),
                customer_revision_id: None,
                contract_revision_id: None,
                snapshot: HeaderSnapshotData {
                    customer_name: "客户".to_string(),
                    contract_no: None,
                    settlement_party_name: None,
                    payment_term_code: "NET-30".to_string(),
                    payment_term_name: "净30天".to_string(),
                    invoice_type: "增值税专用发票".to_string(),
                    tax_point: "13".to_string(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                gross_amount: Amount::from_str("100").unwrap(),
                net_amount: Amount::from_str("100").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                effective_at: Instant::from_unix_secs(1_800_000_000),
                recorded_at: Instant::from_unix_secs(1_800_000_000),
            },
        )
        .unwrap()
    }

    /// 构造销售当前版本公共行。
    fn revision_line(id: &str, stable_line_id: &str) -> SalesOrderRevisionLine {
        SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new(id),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                sales_order_line_id: crate::ids::SalesOrderLineId::new(stable_line_id),
                line_no: 1,
                line_type: LineType::GoodsService,
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                sales_tax_rate: Rate::from_str("0").unwrap(),
                item_name_snapshot: "商品".to_string(),
                spec_snapshot: Some("规格".to_string()),
                unit_snapshot: Some("件".to_string()),
            },
        )
        .unwrap()
    }

    /// 构造销售当前版本商品/服务子类型行。
    fn goods_line(revision_line_id: &str) -> SalesOrderGoodsServiceLineRevision {
        SalesOrderGoodsServiceLineRevision::new(
            crate::ids::SalesOrderGoodsServiceLineRevisionId::new(format!("goods-{revision_line_id}")),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: crate::ids::SkuRevisionId::new("skur-1"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: Quantity::from_str("10").unwrap(),
                base_unit_code: "件".to_string(),
                unit_price_gross: UnitPrice::from_str("5").unwrap(),
            },
        )
        .unwrap()
    }

    /// 构造一条销售覆盖目标行。
    fn coverage_line(stable_line_id: &str, total: &str, covered: &str) -> SalesProcurementCoverageLine {
        SalesProcurementCoverageLine {
            revision_line: revision_line("sorl-1", stable_line_id),
            goods_line: goods_line("sorl-1"),
            product_kind: crate::catalog::ProductKind::Physical,
            summary: ProcurementCoverageSummary::new(
                Quantity::from_str(total).unwrap(),
                Quantity::from_str(covered).unwrap(),
            )
            .unwrap(),
        }
    }

    /// 构造供给稳定身份。
    fn offering(id: &str, supplier_id: &str) -> SupplierOffering {
        SupplierOffering::new(
            SupplierOfferingId::new(id),
            SupplierOfferingData {
                sku_id: SkuId::new("sku-1"),
                supplier_id: SupplierAccountId::new(supplier_id),
                supplier_product_code: None,
                supplier_sku_code: format!("SKU-{id}"),
                source_type: OfferingSourceType::Manual,
                source_connection_id: None,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造供给商业条款修订。
    fn offering_revision(
        offering_id: &str,
        valid_from: &str,
        valid_to: Option<&str>,
    ) -> SupplierOfferingRevision {
        SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new(format!("offrev-{offering_id}")),
            SupplierOfferingRevisionData::from_gross_prices(
                SupplierOfferingId::new(offering_id),
                1,
                UnitPrice::from_str("6").unwrap(),
                UnitPrice::from_str("5").unwrap(),
                Rate::from_str("0.13").unwrap(),
                None,
                None,
                None,
                Quantity::from_str("1").unwrap(),
                vec!["全国".to_string()],
                Vec::new(),
                crate::common::time::BusinessDate::from_str(valid_from).unwrap(),
                valid_to.map(|value| crate::common::time::BusinessDate::from_str(value).unwrap()),
                PrefillSourceRefs {
                    input_tax_rate: None,
                    supply_region: None,
                    valid_from_date: None,
                    valid_from_timezone: None,
                    valid_from_calendar_version: None,
                },
            ),
        )
        .unwrap()
    }

    /// 构造供给实时可供投影。
    fn availability(offering_id: &str, quantity: Option<&str>) -> SupplierOfferingAvailability {
        SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new(format!("avail-{offering_id}")),
            SupplierOfferingAvailabilityData {
                supplier_offering_id: SupplierOfferingId::new(offering_id),
                availability_status: AvailabilityStatus::Available,
                available_quantity: quantity.map(|value| Quantity::from_str(value).unwrap()),
                source_updated_at: Instant::from_unix_secs(1_800_000_000),
                received_at: Instant::from_unix_secs(1_800_000_000),
                source_revision_token: None,
                updated_by: "test".to_string(),
            },
        )
        .unwrap()
    }

    /// 构造一条合格供给。
    fn line_supply(offering_id: &str, supplier_id: &str) -> LineSupply {
        LineSupply {
            offering: offering(offering_id, supplier_id),
            revision: offering_revision(offering_id, "2026-01-01", None),
            availability: availability(offering_id, Some("8")),
        }
    }

    /// 构造销售稳定单。
    fn sales_order(id: &str) -> crate::sales_order::SalesOrder {
        let mut order = crate::sales_order::SalesOrder::new(
            SalesOrderId::new(id),
            crate::sales_order::SalesOrderData {
                order_no: format!("SO-{id}"),
                business_type: crate::sales_order::BusinessType::GoodsService,
                origin_system: crate::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: crate::ids::CustomerAccountId::new("customer-1"),
                contract_id: None,
                settlement_party_id: crate::ids::PartyId::new("party-1"),
                source_status_code: None,
            },
            "seller-1",
        )
        .unwrap();
        order.commercial_status = CommercialStatus::Effective;
        order.procurement_guard_version = 3;
        order
    }

    /// 构造一条完整精确依据分组。
    fn basis_group(supplier_id: &str, payment_term_code: &str) -> BasisGroup {
        let supply = line_supply("offering-1", supplier_id);
        let line = BasisLine {
            coverage: coverage_line("sol-1", "10", "2"),
            supply,
            max_create_quantity: Quantity::from_str("8").unwrap(),
        };
        BasisGroup {
            revision: revision("1"),
            scope: BasisScope {
                supplier_id: SupplierAccountId::new(supplier_id),
                purchase_type: PurchaseType::Physical,
                payment_term_code: payment_term_code.to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
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

    /// 同一范围拆向不同目标仓时必须形成不同的数据库唯一身份。
    #[test]
    fn target_warehouse_scopes_creation_basis_identity() {
        let common_parts = vec!["guard-1".to_string(), "scope-1".to_string()];
        let first = compose_basis_id(
            "so-1",
            common_parts.clone(),
            Some(&WarehouseId::new("warehouse-1")),
        );
        let second = compose_basis_id(
            "so-1",
            common_parts.clone(),
            Some(&WarehouseId::new("warehouse-2")),
        );
        let selectable = compose_basis_id("so-1", common_parts, None);

        assert_ne!(first, second);
        assert_ne!(first, selectable);
        assert_ne!(second, selectable);
    }

    /// 精确范围同时区分供应商、类型、付款条件和履约责任。
    #[test]
    fn basis_scope_key_contains_exact_split_dimensions() {
        let scope = BasisScope {
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
        };

        assert_eq!(basis_scope_key(&scope), "sup-1|PHYSICAL|NET-30|WAREHOUSE");
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

        let mut other_responsibility = base.clone();
        other_responsibility.scope.fulfillment_responsibility = FulfillmentResponsibility::SupplierDirect;
        assert_ne!(base_id, basis_id_for(&order, &other_responsibility, "wi-1", None));

        assert_ne!(base_id, basis_id_for(&order, &base, "wi-2", None));
    }

    /// 商品稳定类型决定采购类型，销售字段不得参与。
    #[test]
    fn product_kind_determines_purchase_type() {
        assert_eq!(
            purchase_type_from_product_kind(crate::catalog::ProductKind::Physical).unwrap(),
            PurchaseType::Physical
        );
        assert_eq!(
            purchase_type_from_product_kind(crate::catalog::ProductKind::Virtual).unwrap(),
            PurchaseType::Virtual
        );
        assert_eq!(
            purchase_type_from_product_kind(crate::catalog::ProductKind::OfflineService).unwrap(),
            PurchaseType::Service
        );
        assert!(purchase_type_from_product_kind(crate::catalog::ProductKind::Voucher).is_err());
    }

    /// 实物由采购选择入仓或直发，其他商品类型只允许其固有路线。
    #[test]
    fn product_kind_limits_fulfillment_options() {
        assert_eq!(
            fulfillment_options(crate::catalog::ProductKind::Physical).unwrap(),
            &[
                FulfillmentResponsibility::Warehouse,
                FulfillmentResponsibility::SupplierDirect,
            ]
        );
        assert_eq!(
            fulfillment_options(crate::catalog::ProductKind::Virtual).unwrap(),
            &[FulfillmentResponsibility::Electronic]
        );
        assert_eq!(
            fulfillment_options(crate::catalog::ProductKind::OfflineService).unwrap(),
            &[FulfillmentResponsibility::Service]
        );
        assert!(fulfillment_options(crate::catalog::ProductKind::Voucher).is_err());
    }

    /// 有限可供量不足销售剩余时允许形成部分数量。
    #[test]
    fn limited_availability_uses_partial_quantity() {
        let remaining = Quantity::from_str("10").unwrap();
        let available = Quantity::from_str("3.5").unwrap();

        assert_eq!(
            maximum_create_quantity(remaining, Some(available)).unwrap(),
            available
        );
        assert_eq!(maximum_create_quantity(remaining, None).unwrap(), remaining);
        assert!(maximum_create_quantity(remaining, Some(Quantity::from_str("-1").unwrap())).is_err());
    }

    /// 入仓取集采价，其他履约责任取一件代发价。
    #[test]
    fn supply_cost_follows_fulfillment_responsibility() {
        let revision = offering_revision("offering-1", "2026-01-01", None);

        assert_eq!(
            supply_cost(&revision, FulfillmentResponsibility::Warehouse),
            UnitPrice::from_str("5").unwrap()
        );
        assert_eq!(
            supply_cost(&revision, FulfillmentResponsibility::SupplierDirect),
            UnitPrice::from_str("6").unwrap()
        );
        assert_eq!(
            supply_cost(&revision, FulfillmentResponsibility::Electronic),
            UnitPrice::from_str("6").unwrap()
        );
        assert_eq!(
            supply_cost(&revision, FulfillmentResponsibility::Service),
            UnitPrice::from_str("6").unwrap()
        );
    }

    /// 请求行解析去除空白并校验数量与交付日。
    #[test]
    fn requested_line_parse_normalizes_and_validates() {
        let line = RequestedLine::parse(" sol-1 ", " 2.5 ", " 2026-08-25 ").unwrap();

        assert_eq!(line.sales_order_line_id, "sol-1");
        assert_eq!(line.quantity, Quantity::from_str("2.5").unwrap());
        assert_eq!(
            line.expected_delivery_date,
            crate::common::time::BusinessDate::from_str("2026-08-25").unwrap()
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
            normalized
                .iter()
                .map(|line| line.sales_order_line_id.clone())
                .collect::<Vec<_>>(),
            vec!["sol-a".to_string(), "sol-b".to_string()]
        );
        assert!(normalize_requested_lines(&[b, duplicate]).is_err());
        assert_eq!(
            normalize_requested_lines(&[]).unwrap(),
            Vec::<RequestedLine>::new()
        );
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
}
