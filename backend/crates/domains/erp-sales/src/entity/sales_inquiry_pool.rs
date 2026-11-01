//! 公司商品池询价模板合同；展示列、需求白名单与版本能力由销售领域统一维护。

use crate::{Error, Result};

/// 公司商品池选品工作表。
pub const POOL_SHEET: &str = "选品";
/// 客户配送工作表，商品池和独立客户模板共用。
pub const DELIVERY_SHEET: &str = "配送需求";
/// 模板版本及填写说明工作表。
pub const INSTRUCTIONS_SHEET: &str = "填写说明";
/// 选品表的稳定结构化引用名称。
pub const POOL_TABLE_NAME: &str = "InquirySelection";
/// 原导出的完整展示列顺序；展示字段不因出现在此处而取得导入写入资格。
pub const POOL_HEADERS: [&str; 20] = [
    "主图",
    "商品名称",
    "规格",
    "SKU 编号",
    "SPU 编号",
    "一件代发价（含税）",
    "集采价（含税）",
    "市场价（含税）",
    "集采起订量",
    "供应商编号",
    "供应保障",
    "商品类型",
    "基础单位",
    "条码",
    "需求数量",
    "销售价",
    "需求备注",
    "询价说明",
    "单位编码",
    "配送关联",
];
/// 允许删除展示列时仍须保留的需求列。
pub const POOL_REQUIRED_HEADERS: [&str; 5] = ["SKU 编号", "单位编码", "需求数量", "销售价", "需求备注"];
/// 需求解析器接收的唯一字段顺序，不包含供应商、展示价格和配送关联。
const REQUIREMENT_HEADERS: [&str; 7] =
    ["SKU 编号", "商品名称", "规格", "单位编码", "需求数量", "销售价", "需求备注"];
/// 自动配送页的业务列；辅助核对列另按模板版本附加。
pub const POOL_DELIVERY_HEADERS: [&str; 12] = [
    "商品名称",
    "SKU 编号",
    "SPU 编号",
    "收件人",
    "联系电话",
    "省",
    "市",
    "区县",
    "详细地址",
    "配送数量",
    "要求交付日期",
    "交付条件",
];
/// 从选品页重算配送自动列所需的完整来源。
pub const POOL_DELIVERY_SOURCE_HEADERS: [&str; 5] =
    ["配送关联", "商品名称", "SKU 编号", "SPU 编号", "需求数量"];
/// v4 的辅助列，只作填写提示，禁止作为需求或数量事实。
const CHECK_HEADERS: [&str; 6] = ["需求数量", "已分配数量", "剩余数量", "分配状态", "核对数量", "核对商品"];
/// 非法选品列布局的固定提示。
pub const INVALID_POOL_HEADERS: &str = "选品页表头无效；可删除展示列，但须保留 SKU 编号、单位编码、需求数量、销售价及需求备注，且不能新增列或重复表头";

/// 已发布商品池模板的读取能力；新增版本不得改变历史版本含义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolTemplateVersion {
    V1,
    V2,
    V3,
    V4,
}

impl PoolTemplateVersion {
    /// 当前导出版本。
    pub const CURRENT: Self = Self::V4;

    /// 读取模板版本标识。
    /// # 参数
    /// `marker` 为填写说明中的原始版本字符串。
    /// # 返回
    /// 已发布版本返回其能力类型，未知版本返回空值。
    /// # 错误
    /// 无；调用方按模板类型返回既有格式错误。
    pub fn from_marker(marker: &str) -> Option<Self> {
        [Self::V1, Self::V2, Self::V3, Self::V4].into_iter().find(|version| version.marker() == marker)
    }

    /// 取得文件中的稳定版本标识。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回写入文档属性和填写说明的同一字符串。
    /// # 错误
    /// 无。
    pub const fn marker(self) -> &'static str {
        match self {
            Self::V1 => "sales-inquiry-pool-v1",
            Self::V2 => "sales-inquiry-pool-v2",
            Self::V3 => "sales-inquiry-pool-v3",
            Self::V4 => "sales-inquiry-pool-v4",
        }
    }

    /// 判定是否按选品页重算配送自动列。
    /// # 参数
    /// 无。
    /// # 返回
    /// v2 及后续已发布版本返回真，v1 保留原独立配送格式。
    /// # 错误
    /// 无。
    pub const fn automatic_delivery(self) -> bool {
        !matches!(self, Self::V1)
    }

    /// 取得该版本要求的辅助核对表头。
    /// # 参数
    /// 无。
    /// # 返回
    /// v1/v2 不含辅助列，v3 含四列，v4 含六列。
    /// # 错误
    /// 无。
    pub fn check_headers(self) -> &'static [&'static str] {
        match self {
            Self::V1 | Self::V2 => &[],
            Self::V3 => &CHECK_HEADERS[..4],
            Self::V4 => &CHECK_HEADERS,
        }
    }
}

/// 已校验的选品布局，仅输出需求白名单的原列索引。
pub struct PoolSelectionLayout<'a> {
    headers: Vec<&'a str>,
    mapping: [Option<usize>; 7],
}

impl<'a> PoolSelectionLayout<'a> {
    /// 解析可重排、可删除展示列的选品表头。
    /// # 参数
    /// `headers` 为表头文本，公式元数据由文件解析边界单独拒绝。
    /// # 返回
    /// 返回规范列名及固定需求白名单映射，缺失可选展示列映射为空。
    /// # 错误
    /// 缺失必填列、未知列或新旧别名同时出现时返回既有表头错误。
    pub fn new(headers: &[&'a str]) -> Result<Self> {
        let headers = headers
            .iter()
            .map(|name| if *name == "销售参考价" { "销售价" } else { *name })
            .collect::<Vec<_>>();
        if headers.iter().any(|name| !POOL_HEADERS.contains(name))
            || POOL_REQUIRED_HEADERS.iter().any(|name| !headers.contains(name))
            || headers.iter().enumerate().any(|(index, name)| headers[..index].contains(name))
        {
            return Err(Error::ValidationError(INVALID_POOL_HEADERS.into()));
        }
        let mapping = REQUIREMENT_HEADERS.map(|name| headers.iter().position(|header| *header == name));
        Ok(Self { headers, mapping })
    }

    /// 取得用于单元格错误定位的规范表头。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回原列顺序的表头，历史销售价别名已规范化。
    /// # 错误
    /// 无。
    pub fn headers(&self) -> &[&str] {
        &self.headers
    }

    /// 取得允许进入询价需求的原列索引。
    /// # 参数
    /// 无。
    /// # 返回
    /// 依次返回 SKU、名称、规格、单位、数量、销售价及备注映射。
    /// # 错误
    /// 无。
    pub fn mapping(&self) -> &[Option<usize>; 7] {
        &self.mapping
    }
}

/// 构造系统允许重算的配送联动公式，供导出与导入精确匹配同一合同。
/// # 参数
/// `key` 为选品稳定配送关联，`field` 为调用方确定的来源字段。
/// # 返回
/// 返回无前导等号的结构化引用表达式，不执行公式。
/// # 错误
/// 无；导入仍须验证完整表达式及关联字段。
pub fn automatic_delivery_formula(key: &str, field: &str) -> String {
    let lookup =
        |column| format!("INDEX({POOL_TABLE_NAME}[{column}],MATCH(\"{key}\",{POOL_TABLE_NAME}[配送关联],0))");
    format!("IF({}=\"\",\"\",{})", lookup("需求数量"), lookup(field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_layout_keeps_only_requirement_fields_and_normalizes_price_alias() {
        let layout = PoolSelectionLayout::new(&[
            "供应商编号",
            "销售参考价",
            "单位编码",
            "SKU 编号",
            "需求数量",
            "需求备注",
            "集采价（含税）",
        ])
        .unwrap();
        assert_eq!(layout.mapping(), &[Some(3), None, None, Some(2), Some(4), Some(1), Some(5)]);
        assert_eq!(layout.headers()[1], "销售价");
    }

    #[test]
    fn selection_layout_rejects_missing_unknown_and_duplicate_alias_headers() {
        for headers in [
            vec!["SKU 编号"],
            [POOL_REQUIRED_HEADERS.as_slice(), &["供应商原始报价"]].concat(),
            [POOL_REQUIRED_HEADERS.as_slice(), &["需求数量"]].concat(),
            [POOL_REQUIRED_HEADERS.as_slice(), &["销售参考价"]].concat(),
        ] {
            assert!(
                matches!(PoolSelectionLayout::new(&headers), Err(Error::ValidationError(message)) if message == INVALID_POOL_HEADERS)
            );
        }
    }

    #[test]
    fn published_template_versions_keep_their_original_capabilities() {
        for (version, automatic, checks) in [
            (PoolTemplateVersion::V1, false, 0),
            (PoolTemplateVersion::V2, true, 0),
            (PoolTemplateVersion::V3, true, 4),
            (PoolTemplateVersion::V4, true, 6),
        ] {
            assert_eq!(PoolTemplateVersion::from_marker(version.marker()), Some(version));
            assert_eq!(version.automatic_delivery(), automatic);
            assert_eq!(version.check_headers().len(), checks);
        }
        assert_eq!(PoolTemplateVersion::from_marker("sales-inquiry-pool-v5"), None);
    }
}
