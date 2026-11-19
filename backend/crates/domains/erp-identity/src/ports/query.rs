use std::sync::Arc;

use mongodb::bson::{Bson, Document};

use super::AccessMapping;

/// 单个事实条件。存在性关联不会在准备阶段过滤根记录。
#[derive(Debug, Clone)]
pub enum QueryFragment {
    Match(Document),
    Exists { collection: String, bindings: Document, pipeline: Vec<Document> },
}

/// 查询编译使用的来源关系描述，承载根记录引用的完整 ID 集合以及对应来源模型。
pub struct RelationMapping {
    /// 产出所有来源 ID 的 MongoDB 数组表达式；缺失和 `null` 按空集合处理。
    pub expected_ids: Bson,
    /// 启动时为该关系登记的来源映射，供编译器生成来源有效性及访问范围条件。
    pub mapping: Arc<dyn AccessMapping>,
}
