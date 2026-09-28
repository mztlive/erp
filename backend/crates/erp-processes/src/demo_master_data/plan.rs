//! 演示主数据的固定清单与生成、删除顺序。
//!
//! 清单只描述要写入的主数据。销售单、采购单、库存和票款不在其中。

use std::collections::HashSet;

use serde::Serialize;

/// 单次接口处理的主数据条数，避免一次请求写太久。
pub(super) const CHUNK_LEN: usize = 8;

const UNITS: &[(&str, &str, &str)] = &[
    ("DEMO-MD-U-PIECE", "件", "件"),
    ("DEMO-MD-U-BOX", "盒", "盒"),
    ("DEMO-MD-U-CASE", "箱", "箱"),
    ("DEMO-MD-U-SET", "套", "套"),
    ("DEMO-MD-U-BOTTLE", "瓶", "瓶"),
    ("DEMO-MD-U-BAG", "袋", "袋"),
];

const BRAND_COUNT: u16 = 8;
const CATEGORY_COUNT: u16 = 10;
const WAREHOUSE_COUNT: u16 = 6;
const CUSTOMER_COUNT: u16 = 24;
const SUPPLIER_COUNT: u16 = 16;
const PRODUCT_COUNT: u16 = 24;

/// 演示主数据种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DemoKind {
    /// 计量单位。
    Unit,
    /// 品牌。
    Brand,
    /// 商品分类。
    Category,
    /// 仓库。
    Warehouse,
    /// 客户。
    Customer,
    /// 供应商。
    Supplier,
    /// 商品。
    Product,
}

impl DemoKind {
    /// 返回清单里保存的种类代码。
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Brand => "brand",
            Self::Category => "category",
            Self::Warehouse => "warehouse",
            Self::Customer => "customer",
            Self::Supplier => "supplier",
            Self::Product => "product",
        }
    }

    /// 从清单种类代码还原种类。
    pub(super) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "unit" => Self::Unit,
            "brand" => Self::Brand,
            "category" => Self::Category,
            "warehouse" => Self::Warehouse,
            "customer" => Self::Customer,
            "supplier" => Self::Supplier,
            "product" => Self::Product,
            _ => return None,
        })
    }
}

/// 各类主数据的条数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DemoCounts {
    /// 计量单位。
    pub unit: u32,
    /// 品牌。
    pub brand: u32,
    /// 分类。
    pub category: u32,
    /// 仓库。
    pub warehouse: u32,
    /// 客户。
    pub customer: u32,
    /// 供应商。
    pub supplier: u32,
    /// 商品。
    pub product: u32,
}

impl DemoCounts {
    fn slot(&mut self, kind: DemoKind) -> &mut u32 {
        match kind {
            DemoKind::Unit => &mut self.unit,
            DemoKind::Brand => &mut self.brand,
            DemoKind::Category => &mut self.category,
            DemoKind::Warehouse => &mut self.warehouse,
            DemoKind::Customer => &mut self.customer,
            DemoKind::Supplier => &mut self.supplier,
            DemoKind::Product => &mut self.product,
        }
    }

    fn add(&mut self, kind: DemoKind) {
        *self.slot(kind) += 1;
    }
}

/// 一条待生成的演示主数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DemoStep {
    /// 种类。
    pub kind: DemoKind,
    /// 种类内序号，从 1 开始。单位清单使用 0，身份在 `key`。
    pub ordinal: u16,
    /// 稳定身份。重复生成和删除都认这个值。
    pub key: String,
}

/// 按固定顺序展开全部演示主数据。
///
/// # 返回
/// 先字典和仓库，再客户、供应商，最后商品。
pub(super) fn demo_steps() -> Vec<DemoStep> {
    let mut steps = Vec::new();
    for (index, (code, _, _)) in UNITS.iter().enumerate() {
        steps.push(DemoStep { kind: DemoKind::Unit, ordinal: (index as u16) + 1, key: (*code).to_string() });
    }
    push_numbered(&mut steps, DemoKind::Brand, BRAND_COUNT, brand_key);
    push_numbered(&mut steps, DemoKind::Category, CATEGORY_COUNT, category_key);
    push_numbered(&mut steps, DemoKind::Warehouse, WAREHOUSE_COUNT, warehouse_key);
    push_numbered(&mut steps, DemoKind::Customer, CUSTOMER_COUNT, customer_key);
    push_numbered(&mut steps, DemoKind::Supplier, SUPPLIER_COUNT, supplier_key);
    push_numbered(&mut steps, DemoKind::Product, PRODUCT_COUNT, product_key);
    steps
}

/// 计划生成的各类条数。
///
/// # 返回
/// 返回固定清单的条数，不读取数据库。
pub fn planned_counts() -> DemoCounts {
    DemoCounts {
        unit: UNITS.len() as u32,
        brand: u32::from(BRAND_COUNT),
        category: u32::from(CATEGORY_COUNT),
        warehouse: u32::from(WAREHOUSE_COUNT),
        customer: u32::from(CUSTOMER_COUNT),
        supplier: u32::from(SUPPLIER_COUNT),
        product: u32::from(PRODUCT_COUNT),
    }
}

/// 统计清单里仍在列表中和已删除的条数。
///
/// # 参数
/// * `rows` - 种类，以及是否已从列表删除
///
/// # 返回
/// 返回 `(仍在列表中, 已删除)`。
pub(super) fn count_records(rows: &[(DemoKind, bool)]) -> (DemoCounts, DemoCounts) {
    let mut active = DemoCounts::default();
    let mut removed = DemoCounts::default();
    for (kind, is_removed) in rows {
        if *is_removed {
            removed.add(*kind);
        } else {
            active.add(*kind);
        }
    }
    (active, removed)
}

/// 取本轮要生成的下标区间。
///
/// # 参数
/// * `cursor` - 上一次返回的下标
/// * `len` - 清单总条数
///
/// # 返回
/// 返回本轮下标，区间不超过 [`CHUNK_LEN`]。
pub(super) fn apply_window(cursor: usize, len: usize) -> std::ops::Range<usize> {
    let start = cursor.min(len);
    let end = start.saturating_add(CHUNK_LEN).min(len);
    start..end
}

/// 取下一批要删除的演示身份，只包含清单里仍有效的记录。
///
/// # 参数
/// * `steps` - 固定清单
/// * `active_keys` - 仍在列表中的演示身份
/// * `limit` - 本轮最多删除的条数
///
/// # 返回
/// 按生成顺序的逆序返回。商品先于字典。清单之外的身份不会出现。
/// 清单里有、但固定计划已经不再包含的身份排在最后，避免旧记录删不掉。
pub(super) fn removal_batch(steps: &[DemoStep], active_keys: &[String], limit: usize) -> Vec<String> {
    let active: HashSet<&str> = active_keys.iter().map(String::as_str).collect();
    let mut ordered = Vec::new();
    for step in steps.iter().rev() {
        if active.contains(step.key.as_str()) {
            ordered.push(step.key.clone());
        }
    }
    for key in active_keys {
        if !steps.iter().any(|step| step.key == *key) {
            ordered.push(key.clone());
        }
    }
    ordered.truncate(limit);
    ordered
}

/// 返回计量单位的名称和符号。
pub(super) fn unit_spec(key: &str) -> Option<(&'static str, &'static str)> {
    UNITS.iter().find(|item| item.0 == key).map(|item| (item.1, item.2))
}

/// 返回商品要引用的单位、品牌和分类身份。
pub(super) fn product_links(ordinal: u16) -> (&'static str, String, String) {
    let brand_ordinal = ((ordinal - 1) % BRAND_COUNT) + 1;
    let category_ordinal = ((ordinal - 1) % CATEGORY_COUNT) + 1;
    (UNITS[0].0, brand_key(brand_ordinal), category_key(category_ordinal))
}

pub(super) fn brand_key(ordinal: u16) -> String {
    format!("DEMO-MD-B-{ordinal:02}")
}

pub(super) fn category_key(ordinal: u16) -> String {
    format!("DEMO-MD-C-{ordinal:02}")
}

pub(super) fn warehouse_key(ordinal: u16) -> String {
    format!("DEMO-MD-W-{ordinal:02}")
}

pub(super) fn customer_key(ordinal: u16) -> String {
    format!("demo-master-customer-{ordinal:02}")
}

pub(super) fn supplier_key(ordinal: u16) -> String {
    format!("demo-master-supplier-{ordinal:02}")
}

pub(super) fn product_key(ordinal: u16) -> String {
    format!("DEMO-MD-P-{ordinal:02}")
}

pub(super) fn supplier_party_no(ordinal: u16) -> String {
    format!("DEMO-MD-PTY-{ordinal:02}")
}

pub(super) fn supplier_no(ordinal: u16) -> String {
    format!("DEMO-MD-SUP-{ordinal:02}")
}

pub(super) fn sku_no(ordinal: u16) -> String {
    format!("DEMO-MD-SKU-{ordinal:02}")
}

fn push_numbered(steps: &mut Vec<DemoStep>, kind: DemoKind, count: u16, key_of: fn(u16) -> String) {
    for ordinal in 1..=count {
        steps.push(DemoStep { kind, ordinal, key: key_of(ordinal) });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BRAND_COUNT, CATEGORY_COUNT, CUSTOMER_COUNT, DemoKind, PRODUCT_COUNT, SUPPLIER_COUNT, UNITS,
        WAREHOUSE_COUNT, apply_window, count_records, demo_steps, planned_counts, removal_batch,
    };

    #[test]
    fn plan_counts_and_order_cover_only_master_data() {
        let steps = demo_steps();
        let expected = UNITS.len()
            + usize::from(BRAND_COUNT)
            + usize::from(CATEGORY_COUNT)
            + usize::from(WAREHOUSE_COUNT)
            + usize::from(CUSTOMER_COUNT)
            + usize::from(SUPPLIER_COUNT)
            + usize::from(PRODUCT_COUNT);
        assert_eq!(steps.len(), expected);
        assert_eq!(planned_counts().customer, u32::from(CUSTOMER_COUNT));
        assert_eq!(planned_counts().product, u32::from(PRODUCT_COUNT));
        assert_eq!(steps.first().map(|step| step.kind), Some(DemoKind::Unit));
        assert_eq!(steps.last().map(|step| step.kind), Some(DemoKind::Product));
        let product_at = steps.iter().position(|step| step.kind == DemoKind::Product).unwrap();
        let category_at = steps.iter().position(|step| step.kind == DemoKind::Category).unwrap();
        assert!(category_at < product_at);
        let mut keys = steps.iter().map(|step| step.key.as_str()).collect::<Vec<_>>();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), steps.len());
    }

    #[test]
    fn removal_only_walks_active_demo_keys_from_product_back_to_dictionary() {
        let steps = demo_steps();
        let unit = steps.iter().find(|step| step.kind == DemoKind::Unit).unwrap();
        let product = steps.iter().find(|step| step.kind == DemoKind::Product).unwrap();
        let inactive = steps.iter().find(|step| step.kind == DemoKind::Brand).unwrap();
        let batch =
            removal_batch(&steps, &[unit.key.clone(), product.key.clone(), "legacy-demo-key".to_string()], 8);
        assert_eq!(batch.first().map(String::as_str), Some(product.key.as_str()));
        assert!(batch.iter().any(|key| key == &unit.key));
        assert!(!batch.iter().any(|key| key == &inactive.key));
        assert!(batch.iter().any(|key| key == "legacy-demo-key"));
        assert!(
            batch.iter().position(|key| key == &product.key).unwrap()
                < batch.iter().position(|key| key == &unit.key).unwrap()
        );
    }

    #[test]
    fn counts_keep_removed_rows_out_of_the_active_total() {
        let (active, removed) = count_records(&[
            (DemoKind::Customer, false),
            (DemoKind::Customer, true),
            (DemoKind::Product, false),
        ]);
        assert_eq!(active.customer, 1);
        assert_eq!(removed.customer, 1);
        assert_eq!(active.product, 1);
        assert_eq!(removed.product, 0);
    }

    #[test]
    fn apply_window_stops_at_the_end_of_the_plan() {
        let len = demo_steps().len();
        assert_eq!(apply_window(0, len), 0..8);
        assert_eq!(apply_window(len - 3, len), (len - 3)..len);
        assert_eq!(apply_window(len, len), len..len);
    }
}
