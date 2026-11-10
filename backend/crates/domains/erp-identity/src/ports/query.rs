use std::sync::Arc;

use mongodb::bson::{Bson, Document};

use super::AccessMapping;

/// 单个事实条件。存在性关联不会在准备阶段过滤根记录。
#[derive(Debug, Clone)]
pub enum QueryFragment {
    Match(Document),
    Exists { collection: String, bindings: Document, pipeline: Vec<Document> },
}

/// 根记录保存的完整来源 ID 集合与来源模型。
/// expected_ids 是 MongoDB 数组表达式；缺失和 null 由编译器解释为空集。
pub struct RelationMapping {
    pub expected_ids: Bson,
    pub mapping: Arc<dyn AccessMapping>,
}
