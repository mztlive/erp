use mongodb::bson::{Document, doc};

/// 组装关键词查询时转义正则元字符，按字面量忽略大小写匹配。有值则写入并覆盖同名条件，返回 `true`；
/// `None` 保留原文档并返回 `false`。
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
