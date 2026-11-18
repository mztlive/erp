//! 完整单据查询的实际来源事实；不组合授权规则。

use mongodb::bson::{Bson, Document, doc};

const SETTLEMENT_PREFIX: &str = "supplier_settlement_statement:";

// 组装资金整单来源时，将仓储已读取的实际引用映射为各来源资源主键。
//
// 调用参数：
// sales、purchases 是完整实际来源表达式，valid 表示全部引用已确认存在。
//
// 各资源独立来源数组和完整性事实的预投影阶段。
//
// 失败边界：
// 不执行 I/O，也不判断人员或权限规则。
pub(in crate::finance::funds_scope) fn fields(sales: Bson, purchases: Bson, valid: Bson) -> Document {
    let settlement = doc! { "$filter": { "input": purchases.clone(), "as": "source", "cond": { "$regexMatch": { "input": { "$ifNull": ["$$source", ""] }, "regex": format!("^{SETTLEMENT_PREFIX}") } } } };
    let purchase = doc! { "$filter": { "input": purchases, "as": "source", "cond": { "$not": [{ "$regexMatch": { "input": { "$ifNull": ["$$source", ""] }, "regex": format!("^{SETTLEMENT_PREFIX}") } }] } } };
    doc! { "$set": {
        "__funds_sales_ids": sales,
        "__funds_purchase_ids": purchase,
        "__funds_settlement_ids": { "$map": { "input": settlement, "as": "source", "in": { "$substrBytes": ["$$source", i32::try_from(SETTLEMENT_PREFIX.len()).expect("固定前缀长度"), { "$subtract": [{ "$strLenBytes": "$$source" }, i32::try_from(SETTLEMENT_PREFIX.len()).expect("固定前缀长度")] }] } } },
        "__funds_sources_valid": valid,
    } }
}
