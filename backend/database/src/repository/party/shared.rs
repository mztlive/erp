use entities::ids::PartyId;
use entities::party::EffectiveRecordStatus;
use mongodb::bson::{doc, Document};

/// 构造指定日期生效的 Party 从属事实过滤条件。
///
/// # 参数
/// * `party_id` - 所属 Party ID
/// * `as_of` - 业务日期
///
/// # 返回
/// 返回启用状态且日期落在左闭右开有效期内的查询文档。
pub(super) fn active_fact_filter(
    party_id: &PartyId,
    as_of: entities::common::time::BusinessDate,
) -> Document {
    let mut filter = active_fact_window_filter(as_of);
    filter.insert("party_id", party_id.to_string());
    filter
}

/// 构造指定日期生效的 Party 从属事实公共时间窗过滤条件。
///
/// # 参数
/// * `as_of` - 业务日期
///
/// # 返回
/// 返回启用状态且日期落在左闭右开有效期内的公共查询文档。
pub(super) fn active_fact_window_filter(as_of: entities::common::time::BusinessDate) -> Document {
    let as_of = as_of.to_string();
    doc! {
        "status": EffectiveRecordStatus::Active.as_str(),
        "valid_from": { "$lte": &as_of },
        "$or": [
            { "valid_to": null },
            { "valid_to": { "$gt": &as_of } },
        ],
    }
}

/// 构建排序文档（仓储白名单）。
///
/// `sort_by` 不在 `allowed` 白名单内时回落默认 `created_at`，禁止透传任意
/// 字段名（P2 §2.3）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|candidate| allowed.contains(candidate))
        .unwrap_or("created_at");
    doc! { field: direction }
}

#[cfg(test)]
mod tests {
    use super::{active_fact_window_filter, sort_doc};
    use entities::common::time::BusinessDate;
    use mongodb::bson::doc;

    #[test]
    fn active_fact_window_is_left_closed_and_right_open() {
        let as_of = BusinessDate::from_ymd(2026, 8, 27).expect("测试日期必须合法");

        let filter = active_fact_window_filter(as_of);

        assert_eq!(
            filter
                .get_document("valid_from")
                .expect("必须包含开始日期")
                .get_str("$lte")
                .expect("开始日期必须包含当天"),
            "2026-08-27"
        );
        let valid_to = filter.get_array("$or").expect("必须包含结束日期分支")[1]
            .as_document()
            .expect("结束日期分支必须是文档")
            .get_document("valid_to")
            .expect("必须包含结束日期");
        assert_eq!(
            valid_to.get_str("$gt").expect("结束日期必须排除当天"),
            "2026-08-27"
        );
    }

    #[test]
    fn sort_doc_falls_back_to_created_at_when_field_is_not_whitelisted() {
        assert_eq!(
            sort_doc(Some("revised_at"), false, &["created_at", "party_no"]),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("party_no"), true, &["created_at", "party_no"]),
            doc! { "party_no": 1 }
        );
        assert_eq!(sort_doc(None, false, &["created_at"]), doc! { "created_at": -1 });
    }
}
