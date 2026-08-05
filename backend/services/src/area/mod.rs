//! 行政区划查询。

use std::collections::BTreeMap;
use std::io::Cursor;

use calamine::{open_workbook_from_rs, Reader, Xlsx};
use tokio::sync::OnceCell;

use crate::errors::{Error, Result};

mod dto;

pub use dto::AreaNode;

const AREA_SOURCE_BYTES: &[u8] = include_bytes!("../../assets/t_map_area.xlsx");

static AREA_TREE_CACHE: OnceCell<std::result::Result<Vec<AreaNode>, String>> = OnceCell::const_new();

type AreaNames = BTreeMap<String, String>;
type ChildAreas = BTreeMap<String, AreaNames>;

/// 获取省市区树。
///
/// 首次调用会在阻塞线程解析内嵌资源，后续调用复用进程内缓存。
///
/// # 返回值
/// 返回省市区树结构。
///
/// # 错误
/// 当数据源读取或解析失败时返回错误。
pub async fn area_tree() -> Result<Vec<AreaNode>> {
    let tree = AREA_TREE_CACHE
        .get_or_init(|| async {
            tokio::task::spawn_blocking(load_area_tree)
                .await
                .map_err(|err| format!("区域数据加载任务失败: {err}"))?
                .map_err(|err| format!("区域数据加载失败: {err}"))
        })
        .await;

    match tree {
        Ok(items) => Ok(items.clone()),
        Err(message) => Err(Error::Internal(message.clone())),
    }
}

/// 从内嵌的 Excel 数据源加载行政区树。
fn load_area_tree() -> Result<Vec<AreaNode>> {
    parse_area_tree(AREA_SOURCE_BYTES)
}

/// 解析 XLSX 字节并构建行政区树。
fn parse_area_tree(source: &[u8]) -> Result<Vec<AreaNode>> {
    let cursor = Cursor::new(source);
    let mut workbook: Xlsx<_> =
        open_workbook_from_rs(cursor).map_err(|err| Error::Internal(format!("区域数据解析失败: {err}")))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| Error::Internal("区域数据工作表不存在".to_string()))?
        .map_err(|err| Error::Internal(format!("区域数据工作表读取失败: {err}")))?;

    let rows = range
        .rows()
        .skip(1)
        .filter_map(|row| row.first().zip(row.get(1)))
        .filter_map(|(adcode, name_path)| {
            let adcode = adcode.to_string().trim().to_string();
            let name_path = name_path.to_string();
            if !is_six_digit_code(adcode.as_str()) {
                return None;
            }
            parse_area_name(name_path.as_str()).map(|name| (adcode, name))
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        return Err(Error::Internal("区域数据为空".to_string()));
    }

    Ok(build_tree_from_rows(rows))
}

/// 由平铺行政区行构建树。
fn build_tree_from_rows(rows: Vec<(String, String)>) -> Vec<AreaNode> {
    let (provinces, mut cities_by_province, mut districts_by_city) = group_area_rows(rows);
    provinces
        .into_iter()
        .map(|(code, name)| AreaNode {
            children: build_city_nodes(
                cities_by_province.remove(&code).unwrap_or_default(),
                &mut districts_by_city,
            ),
            code,
            name,
        })
        .collect()
}

/// 按省、市父编码对平铺行分组。
fn group_area_rows(rows: Vec<(String, String)>) -> (AreaNames, ChildAreas, ChildAreas) {
    let mut provinces = AreaNames::new();
    let mut cities_by_province = ChildAreas::new();
    let mut districts_by_city = ChildAreas::new();
    for (code, name) in rows {
        if code.ends_with("0000") {
            provinces.insert(code, name);
            continue;
        }

        if code.ends_with("00") {
            let province_code = format!("{}0000", &code[..2]);
            cities_by_province
                .entry(province_code)
                .or_default()
                .insert(code, name);
            continue;
        }

        let city_code = format!("{}00", &code[..4]);
        districts_by_city.entry(city_code).or_default().insert(code, name);
    }
    (provinces, cities_by_province, districts_by_city)
}

/// 将同一省份的城市及其区县转换为节点。
fn build_city_nodes(cities: AreaNames, districts_by_city: &mut ChildAreas) -> Vec<AreaNode> {
    cities
        .into_iter()
        .map(|(code, name)| AreaNode {
            children: districts_by_city
                .remove(&code)
                .unwrap_or_default()
                .into_iter()
                .map(|(code, name)| AreaNode {
                    code,
                    name,
                    children: Vec::new(),
                })
                .collect(),
            code,
            name,
        })
        .collect()
}

/// 解析行政区名称，取名称路径中的最后一个非空段。
fn parse_area_name(name_path: &str) -> Option<String> {
    name_path
        .split(',')
        .map(str::trim)
        .rfind(|item| !item.is_empty())
        .map(ToString::to_string)
}

/// 判断是否为 6 位数字行政区编码。
fn is_six_digit_code(code: &str) -> bool {
    code.len() == 6 && code.chars().all(|item| item.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_area_name_returns_last_non_empty_segment() {
        assert_eq!(parse_area_name("中国,,北京市,东城区").as_deref(), Some("东城区"));
        assert_eq!(parse_area_name("中国,,北京市").as_deref(), Some("北京市"));
        assert_eq!(parse_area_name("  "), None);
    }

    #[test]
    fn build_tree_handles_municipality_rows() {
        let rows = vec![
            ("110000".to_string(), "北京市".to_string()),
            ("110100".to_string(), "北京市".to_string()),
            ("110101".to_string(), "东城区".to_string()),
            ("110102".to_string(), "西城区".to_string()),
        ];

        let tree = build_tree_from_rows(rows);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].code, "110000");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].code, "110100");
        assert_eq!(tree[0].children[0].children.len(), 2);
    }

    #[test]
    fn embedded_area_source_builds_expected_tree() {
        let tree = load_area_tree().expect("embedded area source should be valid");

        let beijing = tree
            .iter()
            .find(|province| province.code == "110000")
            .expect("Beijing province node should exist");
        let beijing_city = beijing
            .children
            .iter()
            .find(|city| city.code == "110100")
            .expect("Beijing city node should exist");

        assert_eq!(beijing.name, "北京市");
        assert!(beijing_city
            .children
            .iter()
            .any(|district| district.code == "110101"));
    }

    #[test]
    fn invalid_area_source_returns_parse_error() {
        let error = parse_area_tree(b"not an xlsx workbook").expect_err("invalid xlsx should fail");

        assert!(matches!(error, Error::Internal(message) if message.contains("区域数据解析失败")));
    }

    #[tokio::test]
    async fn area_tree_reuses_the_embedded_source() {
        let first = area_tree()
            .await
            .expect("embedded area source should be available");
        let second = area_tree()
            .await
            .expect("cached area tree should remain available");

        assert_eq!(first, second);
    }
}
