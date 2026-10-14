//! 行政区划只读 Handler：供收货地址、仓库地区与配送模板选择地区。

use erp_core::region::all_regions;
use serde::Serialize;

use crate::core::errors::Result;
use crate::core::response::ApiResponse;

/// 一条行政区划。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RegionNodeView {
    /// 6 位代码。
    pub code: &'static str,
    /// 名称。
    pub name: &'static str,
    /// 上级代码；省级为空。
    pub parent_code: Option<&'static str>,
}

/// 返回内置行政区划全集。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回省、市、区县三级区划，按数据顺序排列。
pub async fn list_regions() -> Result<Vec<RegionNodeView>> {
    let regions = all_regions()
        .iter()
        .map(|node| RegionNodeView { code: node.code, name: node.name, parent_code: node.parent_code })
        .collect();
    Ok(ApiResponse::ok_with_data(regions))
}

#[cfg(test)]
mod tests {
    use super::list_regions;

    #[tokio::test]
    async fn regions_include_province_city_and_district() {
        let response = list_regions().await.unwrap();
        let regions = response.data.unwrap();
        assert!(regions.iter().any(|node| node.code == "440000" && node.parent_code.is_none()));
        assert!(regions.iter().any(|node| node.code == "440305" && node.parent_code == Some("440300")));
    }
}
