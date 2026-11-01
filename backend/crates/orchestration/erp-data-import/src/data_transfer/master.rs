//! 主资料导出保留统一九列及原列表派生过滤。
use serde_json::{Value, json};

use super::quotes::date;
use super::workbook::text;

pub(super) fn rows(input: &Value, rows: Vec<Value>) -> Vec<Value> {
    let resource = text(input, "resource");
    rows.into_iter()
        .filter_map(|row| {
            let enabled = resource == "sellable" || text(&row, "status").eq_ignore_ascii_case("active");
            if !matches_filter(&input["_filters"], enabled) {
                return None;
            }
            let mut result = match resource.as_str() {
                "product" => product(&row),
                "sellable" => sellable(&row),
                _ => supplier(&row),
            };
            result["lifecycle"] = json!(if enabled { "启用" } else { "停用" });
            result["timing"] = json!("当前生效");
            result["blocker"] = json!(if enabled { "" } else { "已停用" });
            Some(result)
        })
        .collect()
}
fn matches_filter(filter: &Value, enabled: bool) -> bool {
    // 三个目录均将当前返回版本标记为 CURRENT；未来版本不在列表结果内。
    filter["revision_timing"] != "future"
        && filter["metric"] != "pending"
        && filter["metric"] != "expiring"
        && !(filter["metric"] == "enabled" && !enabled)
        && !(filter["metric"] == "disabled" && enabled)
}
fn product(row: &Value) -> Value {
    let facts = format!(
        "商品编号：{}；商品类型：{}；上架 SKU：{}/{}；有供给 SKU：{}/{}；已填一件代发价 SKU：{}/{}",
        text(row, "product_no"),
        kind(row),
        row["listed_sku_count"],
        row["sku_count"],
        row["supplied_sku_count"],
        row["sku_count"],
        row["priced_sku_count"],
        row["sku_count"]
    );
    json!({"number":row["product_no"],"name":row["name"],"revision":format!("v{}",row["version"]),"from":date(&row["created_at"]),"to":"长期","facts":facts})
}
fn sellable(row: &Value) -> Value {
    let facts = format!(
        "SKU：{}；一件代发价：{}；集采价：{}；市场价：{}；集采起订量：{}；商品编号：{}；基础单位：{}；商品类型：{}；有效供应商：{} 家；供应商编号：{}",
        text(row, "sku_no"),
        text(row, "sales_visible_price_gross"),
        text(row, "bulk_price_gross"),
        text(row, "market_price"),
        text(row, "bulk_min_quantity"),
        text(row, "product_no"),
        text(row, "base_unit_name"),
        kind(row),
        row["supplier_count"],
        array(&row["supplier_codes"])
    );
    json!({"number":row["sku_no"],"name":row["name"],"revision":format!("v{}",row["sku_revision_no"]),"from":row["effective_from"],"to":row["effective_to"].as_str().unwrap_or("长期"),"facts":facts})
}
fn supplier(row: &Value) -> Value {
    let profile = &row["current_profile"];
    let mut facts = vec![
        format!("供应能力：{}", array(&row["capability_codes"])),
        format!("资质类型：{}", array(&row["qualification_types"])),
        format!("资质健康度：{}", text(row, "qualification_health")),
    ];
    for (field, label) in [
        ("settlement_mode", "结算方式"),
        ("payment_term_snapshot", "付款条件"),
        ("signing_entity_name", "签约主体"),
        ("payment_entity_name", "付款主体"),
        ("invoice_type", "发票类型"),
        ("business_category", "经营类目"),
    ] {
        facts.push(format!("{label}：{}", text(profile, field)));
    }
    let rates =
        profile.get("invoice_tax_rates").filter(|v| !v.is_null()).unwrap_or(&profile["invoice_tax_rate"]);
    if !rates.is_null() {
        facts.push(format!("进项税率（小数）：{rates}"));
    }
    let revision = profile.get("revision_no").unwrap_or(&row["version"]);
    json!({"number":row["supplier_no"],"name":row["legal_name"].as_str().or_else(||row["short_name"].as_str()).unwrap_or_default(),"revision":format!("v{revision}"),"from":date(profile.get("created_at").unwrap_or(&row["created_at"])),"to":"长期","facts":facts.join("；")})
}
fn array(value: &Value) -> String {
    value.as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join("、")
}
fn kind(row: &Value) -> &str {
    match row["product_kind"].as_str() {
        Some("PHYSICAL") => "实物",
        Some("SERVICE" | "OFFLINE_SERVICE") => "线下服务",
        Some("VIRTUAL") => "虚拟商品",
        Some("VOUCHER") => "卡券",
        _ => "",
    }
}
