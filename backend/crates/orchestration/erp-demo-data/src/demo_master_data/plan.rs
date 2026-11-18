//! 演示主数据的固定清单与生成、删除顺序。
//!
//! 清单只描述要写入的主数据。销售单、采购单、库存和票款不在其中。

use serde::Serialize;

use super::seed::{self, SeedRequest};
use crate::Result;

/// 单次接口处理的主数据条数，避免一次请求写太久。
pub(super) const CHUNK_LEN: usize = 8;

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
    // 返回写入演示清单的稳定种类代码，供登记和后续统计使用，不使用展示名称作为标识。
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

    // 把清单保存的固定种类代码还原为枚举，使统计可以分组；未知代码返回 `None`。
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
    // 将种类映射到对应计数字段，供计划和实际清单统计使用同一分组口径。
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

    // 按种类累计计划或登记数量，供状态接口对比各类主数据的进度。
    fn add(&mut self, kind: DemoKind) {
        *self.slot(kind) += 1;
    }
}

/// 一条已校验的 JSON 种子。
#[derive(Clone)]
pub(super) struct DemoStep {
    /// 数据种类。
    pub kind: DemoKind,
    /// 业务编号或幂等键。
    pub key: String,
    /// 复用领域创建请求的固定内容。
    pub request: SeedRequest,
}

// 加载内嵌 JSON 并校验字段及引用后按声明顺序返回执行步骤，使依赖项先于使用方准备。
pub(super) fn demo_steps() -> Result<Vec<DemoStep>> {
    seed::load(include_str!("master-data.json"))
}

// 从已校验种子汇总各类计划数量，让状态接口可将应生成条数与登记实况对照。
pub(super) fn planned_counts(steps: &[DemoStep]) -> DemoCounts {
    let mut counts = DemoCounts::default();
    for step in steps {
        counts.add(step.kind);
    }
    counts
}

// 按种类分别统计有效登记与旧版删除登记，状态查询不会把已删除条数算作已生成。
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

// 根据清单下标计算本轮执行区间，长度不超过 `CHUNK_LEN`；游标超过总数时返回空区间，便于结束分批任务。
pub(super) fn apply_window(cursor: usize, len: usize) -> std::ops::Range<usize> {
    let start = cursor.min(len);
    let end = start.saturating_add(CHUNK_LEN).min(len);
    start..end
}
