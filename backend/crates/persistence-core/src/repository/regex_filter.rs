use mongodb::bson::{Document, doc};

/// 写入按字面量匹配且忽略大小写的 MongoDB 正则条件。
///
/// 覆盖语义：`value` 为 `None` 时静默跳过（返回 `false`，不修改 `filter`）；
/// `value` 为 `Some` 时写入（返回 `true`），已存在同名字段会被覆盖。
/// 调用方如需区分“未传值”与“已写入”，请检查返回值。
///
/// # 参数
/// * `filter` - 待写入的查询文档
/// * `field` - 字段名
/// * `value` - 可选字面量；`None` 表示不加条件
///
/// # 返回
/// 是否写入了正则条件。
///
/// # 错误
/// 不返回错误。
pub fn insert_literal_regex_filter(filter: &mut Document, field: &str, value: Option<&str>) -> bool {
    let Some(value) = value else {
        return false;
    };
    filter.insert(
        field,
        doc! {
            "$regex": regex::escape(value),
            "$options": "i",
        },
    );
    true
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Document, doc};

    use super::insert_literal_regex_filter;

    #[test]
    fn escapes_regex_metacharacters_as_literal_text() {
        let mut filter = Document::new();

        assert!(insert_literal_regex_filter(&mut filter, "name", Some("a.b+[x]")));

        assert_eq!(
            filter,
            doc! {
                "name": {
                    "$regex": r"a\.b\+\[x\]",
                    "$options": "i",
                }
            }
        );
    }

    #[test]
    fn none_skips_and_existing_field_is_overwritten() {
        let mut filter = Document::new();

        assert!(!insert_literal_regex_filter(&mut filter, "name", None));
        assert!(filter.is_empty());

        assert!(insert_literal_regex_filter(&mut filter, "name", Some("abc")));
        assert!(insert_literal_regex_filter(&mut filter, "name", Some("def")));
        let doc = filter.get_document("name").unwrap();
        assert_eq!(doc.get_str("$regex").unwrap(), regex::escape("def"));
    }
}
