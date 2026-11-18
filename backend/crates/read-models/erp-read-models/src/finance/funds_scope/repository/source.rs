//! 复用已解析的领域来源条件，保留来源存在与来源授权的区别。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Bson, Document, doc};

use super::super::{FundsAuthorization, FundsLinkedCondition};

impl FundsAuthorization {
    // 组装财务来源范围的聚合管道时，为实际销售来源生成完整授权阶段。
    //
    // 有销售来源时返回其阶段；未登记该来源时拒绝全部销售份额。
    //
    // 失败边界：
    // 无；使用前后仍须复验整个授权。
    pub(super) fn sales_stages(&self) -> Vec<Document> {
        self.sales
            .as_ref()
            .map_or_else(|| vec![doc! { "$match": { "$expr": false } }], |scope| scope.stages().to_vec())
    }
}

// 组装财务来源范围的聚合管道时，关联一个真实来源并分别保留最小责任事实与授权布尔值。
//
// 调用参数：
// * `collection` - 来源集合名。
// * `local_expression` - 原单引用表达式。
// * `as_field` - 来源事实输出字段。
// * `scope` - 已解析来源范围条件。
// * `owner_field` - 来源负责人字段。
// * `number_field` - 来源单号字段。
//
// 关联阶段。`as_field` 为存在来源的事实，`{as_field}_allowed` 为实际范围资格；缺失来源时事实留空且资格为 false。
pub(crate) fn source_stages(
    collection: &str,
    local_expression: Bson,
    as_field: &str,
    scope: Document,
    owner_field: &str,
    number_field: &str,
    grant_lookup: &[Document],
) -> Vec<Document> {
    let owner = owner_expression(owner_field);
    let number = format!("${number_field}");
    let reference = format!("${as_field}");
    let allowed = format!("{as_field}_allowed");

    let mut allowed_stages = grant_lookup.to_vec();
    allowed_stages.extend([doc! { "$match": scope }, doc! { "$project": { "_id": 0, "id": 1 } }]);
    vec![
        doc! { "$lookup": { "from": collection, "let": { "source_id": local_expression },
        "pipeline": [
            { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": { "$eq": ["$id", "$$source_id"] } } },
            { "$facet": {
                "facts": [{ "$project": { "_id": 0, "id": 1, "owner_user_id": owner,
                    "business_org_unit_id": 1, "version": 1, "document_no": number } }],
                "allowed": allowed_stages,
            } },
        ], "as": as_field } },
        doc! { "$set": { as_field: { "$arrayElemAt": [&reference, 0] } } },
        doc! { "$set": { allowed: { "$gt": [{ "$size": format!("{reference}.allowed") }, 0] },
        as_field: { "$arrayElemAt": [format!("{reference}.facts"), 0] } } },
    ]
}

// 组装财务来源范围的聚合管道时，存量采购责任缺省或全空白时沿用领域当前责任事实的 None 口径。
fn owner_expression(field: &str) -> Bson {
    let reference = format!("${field}");
    if field != "owner_user_id" {
        return Bson::String(reference);
    }
    let whitespace = " \t\n\r\u{000b}\u{000c}\u{0085}\u{00a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}";
    doc! { "$cond": [ { "$eq": [ { "$trim": { "input": { "$ifNull": [&reference, ""] },
    "chars": whitespace } }, ""] }, Bson::Null, reference ] }
    .into()
}

// 应用财务来源范围筛选时，业务筛选与来源授权求交，不替换授权分支。
//
// 调用参数：
// * `condition` - 已展开组织和精确身份条件。
// * `owner_field` - 来源负责人字段名。
//
// 负责人和组织的 `$in` 条件；未提供的维度不写入。经办维度不在此文档中。
pub(crate) fn linked_condition_document(condition: &FundsLinkedCondition, owner_field: &str) -> Document {
    let mut document = Document::new();
    if let Some(ids) = &condition.owner_user_ids {
        document.insert(owner_field, doc! { "$in": ids });
    }
    if let Some(ids) = &condition.org_unit_ids {
        document.insert("business_org_unit_id", doc! { "$in": ids });
    }
    document
}
