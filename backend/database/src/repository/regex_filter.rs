use mongodb::bson::{doc, Document};

/// 写入按字面量匹配且忽略大小写的 MongoDB 正则条件。
pub(super) fn insert_literal_regex_filter(filter: &mut Document, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        filter.insert(
            field,
            doc! {
                "$regex": regex::escape(value),
                "$options": "i",
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{doc, Document};

    use super::insert_literal_regex_filter;

    #[test]
    fn escapes_regex_metacharacters_as_literal_text() {
        let mut filter = Document::new();

        insert_literal_regex_filter(&mut filter, "name", Some("a.b+[x]"));

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
}
