//! 消费方已解析的中性责任范围，默认拒绝且不携带任意数据库表达式。
use mongodb::bson::{Document, doc};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InquiryScopeClause {
    pub company: bool,
    pub owner_user_id: Option<String>,
    pub business_org_unit_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InquiryReadScope {
    pub clauses: Vec<InquiryScopeClause>,
    pub limit: Option<InquiryScopeClause>,
}

impl InquiryReadScope {
    /// 创建已由调用方证明的公司范围；不自行授予权限。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回公司责任筛选；入口仍须核验当前账号和功能权限。
    /// # 错误
    /// 无。
    pub fn company() -> Self {
        Self { clauses: vec![InquiryScopeClause { company: true, ..Default::default() }], limit: None }
    }

    pub(super) fn document(&self) -> Document {
        let granted = union(self.clauses.iter().map(InquiryScopeClause::document).collect());
        match &self.limit {
            Some(limit) => doc! { "$and": [granted, limit.document()] },
            None => granted,
        }
    }
}

impl InquiryScopeClause {
    fn document(&self) -> Document {
        if self.company {
            return Document::new();
        }
        let mut alternatives = Vec::new();
        if let Some(owner) = self.owner_user_id.as_deref().map(str::trim).filter(|owner| !owner.is_empty()) {
            alternatives.push(doc! { "sales_owner_user_id": owner });
        }
        if !self.business_org_unit_ids.is_empty() {
            alternatives.push(doc! { "business_org_unit_id": { "$in": &self.business_org_unit_ids } });
        }
        union(alternatives)
    }
}

fn union(conditions: Vec<Document>) -> Document {
    if conditions.is_empty() {
        doc! { "id": { "$in": [] } }
    } else {
        doc! { "$or": conditions }
    }
}
