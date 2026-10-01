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
    /// 取得对应种类的计数位置。
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

    /// 增加一种主数据的计划数量。
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

/// 读取并校验嵌入程序的种子，按 JSON 顺序返回。
///
/// # 参数
/// 无；读取内嵌 JSON。
///
/// # 返回
/// 返回已完成字段和引用校验的种子清单。
///
/// # 错误
/// JSON 或领域输入无效时返回错误。
pub(super) fn demo_steps() -> Result<Vec<DemoStep>> {
    seed::load(include_str!("master-data.json"))
}

/// 从种子清单统计计划条数。
///
/// # 参数
/// `steps` - 已校验的种子清单。
///
/// # 返回
/// 返回各类计划数量。
///
/// # 错误
/// 无。
pub(super) fn planned_counts(steps: &[DemoStep]) -> DemoCounts {
    let mut counts = DemoCounts::default();
    for step in steps {
        counts.add(step.kind);
    }
    counts
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
