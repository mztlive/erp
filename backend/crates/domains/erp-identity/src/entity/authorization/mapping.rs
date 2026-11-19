use std::collections::BTreeSet;

use super::{FactKind, QueryModelKey};
use crate::{Error, Result};

/// 启动时登记的查询模型，固定根集合、行身份、输出粒度及支持的事实维度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryModel {
    /// 查询模型的登记标识，用于绑定授权凭证。
    pub key: QueryModelKey,
    /// 模型对应的业务资源，用于核对授权入口和来源关系。
    pub resource: String,
    /// 查询根记录或关联来源所在的集合。
    pub collection: String,
    /// 结果行的主键字段路径，受限视图也须保留此字段。
    pub id_field: String,
    /// 固定输出粒度；受限视图同时声明可投影的字段。
    pub output: QueryOutput,
    /// 映射能转换为查询条件的事实维度，供启动时校验规则所需能力。
    pub facts: BTreeSet<FactKind>,
}

/// 查询的固定输出粒度；完整对象、单一来源份额与存在性视图各用独立模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryOutput {
    /// 返回完整业务对象。
    Object,
    /// 返回单一来源的可见份额，每行只对应一个来源。
    Share {
        /// 允许投影的份额字段与行主键；金额仅取该份额。
        fields: BTreeSet<String>,
    },
    /// 仅暴露记录存在性的只读视图。
    Existence {
        /// 投影字段集合，必须且只能包含行主键。
        fields: BTreeSet<String>,
    },
}

impl QueryModel {
    /// 登记时校验资源、集合、行主键和投影路径；前三项非空且各最多 128 字节。
    /// 受限输出必须包含行主键，存在性输出只允许该主键。
    pub fn validate(&self) -> Result<()> {
        for value in [&self.resource, &self.collection, &self.id_field] {
            if value.is_empty()
                || value.len() > 128
                || value.starts_with("__identity_")
                || !value.split('.').all(valid_segment)
            {
                return Err(Error::ValidationError("授权查询模型非法".into()));
            }
        }

        if let QueryOutput::Share { fields } | QueryOutput::Existence { fields } = &self.output
            && (!fields.contains(&self.id_field)
                || fields.iter().any(|field| !field.split('.').all(valid_segment)))
        {
            return Err(Error::ValidationError("受限查询必须声明有效字段及行主键".into()));
        }
        if let QueryOutput::Existence { fields } = &self.output
            && fields.len() != 1
        {
            return Err(Error::ValidationError("存在性查询只允许输出行主键".into()));
        }
        Ok(())
    }
}

// 路径片段须由 ASCII 字母、数字或下划线组成，且非空、避开编译器保留前缀 `__identity_`。
fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with("__identity_")
        && value.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证份额模型允许合规投影，受限视图必须包含行主键且字段路径合法。
    #[test]
    fn restricted_models_require_row_identity_and_valid_paths() {
        let mut model = QueryModel {
            key: QueryModelKey::parse("shares").unwrap(),
            resource: "invoice".into(),
            collection: "rows".into(),
            id_field: "id".into(),
            output: QueryOutput::Share { fields: ["id".into(), "amount".into()].into() },
            facts: BTreeSet::new(),
        };
        assert!(model.validate().is_ok());

        model.output = QueryOutput::Existence { fields: ["amount".into()].into() };
        assert!(model.validate().is_err());

        for field in ["a..b", "$field", "nested.__identity_temp", ""] {
            model.output = QueryOutput::Share { fields: ["id".into(), field.into()].into() };
            assert!(model.validate().is_err());
        }
    }
}
