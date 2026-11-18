use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use serde_json::Value;

use super::{FactKind, QueryModelKey};
use crate::{Error, Result};

/// 登记模型的查询行身份；一个模型固定一组记录和粒度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryModel {
    pub key: QueryModelKey,
    pub resource: String,
    pub collection: String,
    pub id_field: String,
    pub output: QueryOutput,
    pub facts: BTreeSet<FactKind>,
}

/// 完整对象、单一来源份额和存在性视图必须登记不同模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryOutput {
    Object,
    Share { fields: BTreeSet<String> },
    Existence { fields: BTreeSet<String> },
}

impl QueryModel {
    /// 校验服务端查询模型。
    /// 标识和路径可用于权限编译时成功。
    ///
    /// 拒绝非法模型或字段、缺少行主键的受限输出，以及包含额外字段的存在性输出。
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

// 登记字段路径时拒绝空片段或占用授权编译器临时字段的片段，避免业务字段和聚合中间值冲突。
fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with("__identity_")
        && value.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

/// 映射准备的修订号和有效期。无修订号时只能用于调用方事务快照。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MappingFacts {
    /// 当前调用冻结的数据，Mapping 不得把准备结果写入共享实例。
    pub data: Value,
    pub version: Option<String>,
    pub valid_until: Option<Instant>,
}

impl MappingFacts {
    /// 核验事实是否具有本执行方式所需的一致性证据。
    /// 事实可继续使用时成功。
    ///
    /// 无版本的非事务读取、空版本或已经到期拒绝。
    ///
    /// transactional 表示使用调用方事务；now 为复验时间。
    pub fn validate(&self, transactional: bool, now: Instant) -> Result<()> {
        if self.version.as_ref().is_some_and(|value| value.is_empty())
            || (!transactional && self.version.is_none())
        {
            return Err(Error::ValidationError("非事务授权事实必须提供有效修订号".into()));
        }
        if self.valid_until.is_some_and(|until| now >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：授权事实已到期".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn facts_require_revision_or_transaction_and_expire_at_boundary() {
        let now = Instant::from_unix_secs(10);
        let mut facts = MappingFacts::default();
        assert!(facts.validate(false, now).is_err());
        assert!(facts.validate(true, now).is_ok());
        facts.version = Some(String::new());
        assert!(facts.validate(true, now).is_err());

        facts.version = Some("v1".into());
        facts.valid_until = Some(Instant::from_unix_secs(11));
        assert!(facts.validate(false, now).is_ok());
        assert!(matches!(facts.validate(false, Instant::from_unix_secs(11)), Err(Error::ConflictError(_))));
    }
}
