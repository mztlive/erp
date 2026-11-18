use mongodb::bson::{Document, doc};

use crate::ports::QueryFragment;
use crate::{Error, Result};

/// 一个集合查询内的关联准备和最终条件；嵌套来源使用各自的字段作用域。
#[derive(Default)]
pub(super) struct Fragment {
    pub(super) stages: Vec<Document>,
    lookups: Vec<(Vec<u8>, Document)>,
    pub(super) filter: Document,
}

impl Fragment {
    pub(super) fn with_stages(stages: Vec<Document>) -> Self {
        Self { stages, ..Self::default() }
    }

    // 向同一集合管道登记关联并复用完全相同的 lookup；临时别名不跨来源子管道。
    pub(super) fn lookup(&mut self, lookup: Document) -> Result<String> {
        // BSON 文档的 PartialEq 忽略字段顺序，不能用于合并含排序或嵌入文档比较的管道。
        let bytes = lookup.to_vec().map_err(|_| Error::ValidationError("授权关联无法编码为 BSON".into()))?;
        let index = match self.lookups.iter().position(|(existing, _)| *existing == bytes) {
            Some(index) => index,
            None => {
                let index = self.lookups.len();
                self.lookups.push((bytes, lookup));
                index
            },
        };
        Ok(format!("__identity_fact_{index}"))
    }

    // 把业务事实映射成谓词或关联存在性条件，准备关联数组时保留所有根记录供最终逻辑过滤。
    pub(super) fn fact(&mut self, fragment: QueryFragment) -> Result<Document> {
        match fragment {
            QueryFragment::Match(filter) => Ok(filter),
            QueryFragment::Exists { collection, bindings, mut pipeline } => {
                if collection.is_empty() || collection.contains('$') {
                    return Err(Error::ValidationError("事实关联集合非法".into()));
                }
                pipeline.push(doc! { "$limit": 1 });
                let alias =
                    self.lookup(doc! { "from": collection, "let": bindings, "pipeline": pipeline })?;
                Ok(exists(&alias))
            },
        }
    }

    // 关联都已登记后生成最终授权过滤与临时字段清理阶段；别名只在当前集合的条件求值期间存在。
    pub(super) fn finish(self) -> Vec<Document> {
        let mut stages = self.stages;
        let mut aliases = Vec::new();
        for (index, (_, mut lookup)) in self.lookups.into_iter().enumerate() {
            let alias = format!("__identity_fact_{index}");
            lookup.insert("as", &alias);
            stages.push(doc! { "$lookup": lookup });
            aliases.push(alias);
        }

        stages.push(doc! { "$match": self.filter });
        if !aliases.is_empty() {
            stages.push(doc! { "$unset": aliases });
        }
        stages
    }
}

// 用数组是否存在来判断关联命中，不展开元素或改变根记录条数。
pub(super) fn exists(alias: &str) -> Document {
    doc! { format!("{alias}.0"): { "$exists": true } }
}

// 组装完整授权谓词的与或分支；没有条件时拒绝，不把空集合解释为无范围限制。
pub(super) fn combine(operator: &str, conditions: Vec<Document>) -> Document {
    if conditions.is_empty() {
        doc! { "$expr": false }
    } else {
        doc! { operator: conditions }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::execute;
    use super::*;

    #[test]
    fn lookup_reuse_preserves_bson_field_order_and_rejects_invalid_encoding() {
        let mut fragment = Fragment::default();
        let lookup = |sort: Document| {
            doc! {
                "from": "sources", "let": {}, "pipeline": [{ "$sort": sort }, { "$limit": 1 }]
            }
        };

        let first = fragment.lookup(lookup(doc! { "priority": 1, "created_at": 1 })).unwrap();
        let second = fragment.lookup(lookup(doc! { "created_at": 1, "priority": 1 })).unwrap();
        assert_ne!(first, second);
        assert_eq!(fragment.lookup(lookup(doc! { "priority": 1, "created_at": 1 })).unwrap(), first);

        assert!(matches!(
            fragment.lookup(lookup(doc! { "invalid\0field": 1 })),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn lookup_reuse_preserves_different_bindings_predicates_and_collections() {
        let mut fragment = Fragment::default();
        let fact = |collection: &str, binding: &str, user: &str| QueryFragment::Exists {
            collection: collection.into(),
            bindings: doc! { "target": binding },
            pipeline: vec![doc! { "$match": {
                "user": user, "$expr": { "$eq": ["$target", "$$target"] }
            } }],
        };

        let first = fragment.fact(fact("participants", "$id", "u")).unwrap();
        let duplicate = fragment.fact(fact("participants", "$id", "u")).unwrap();
        let other_binding = fragment.fact(fact("participants", "$other", "u")).unwrap();
        let other_user = fragment.fact(fact("participants", "$id", "other")).unwrap();
        let other_collection = fragment.fact(fact("approvers", "$id", "u")).unwrap();
        fragment.filter =
            combine("$and", vec![first, duplicate, other_binding, other_user, other_collection]);
        let stages = fragment.finish();
        assert_eq!(stages.iter().filter(|stage| stage.contains_key("$lookup")).count(), 4);

        let rows = (1..=4).map(|i| doc! { "id": format!("r{i}"), "other": format!("a{i}") }).collect();
        let participants = [
            ("r1", "u"),
            ("a1", "u"),
            ("r1", "other"),
            ("r2", "u"),
            ("r2", "other"),
            ("r3", "u"),
            ("a3", "u"),
            ("r4", "u"),
            ("a4", "u"),
            ("r4", "other"),
        ]
        .iter()
        .map(|(target, user)| doc! { "target": *target, "user": *user })
        .collect();
        let approvers = (1..=3).map(|i| doc! { "target": format!("r{i}"), "user": "u" }).collect();
        let collections = [("participants".into(), participants), ("approvers".into(), approvers)].into();

        assert_eq!(execute(rows, &stages, &collections, &doc! {}), vec![doc! { "id": "r1", "other": "a1" }]);
    }
}
