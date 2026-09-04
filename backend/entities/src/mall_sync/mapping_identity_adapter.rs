//! 快照来源身份适配：规范化快照 JSON → 注册字段候选 → 唯一外部身份。
//!
//! JSON 标量形态（字符串/整数）与注册字段优先级属于领域解释口径，
//! 由本域值对象独占；传输错误类别仍由调用方按失配原因映射。

use serde_json::Value;

use crate::errors::Error;

use super::master_mapping_task::{MappingSourceIdentity, MappingTaskType};

/// 快照来源身份解析失配的强类型原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotIdentityError {
    /// 映射类型未注册可解析的外部规范身份。
    Unregistered,
    /// 规范化快照不是可验证的 JSON 对象。
    NotJsonObject,
    /// 快照来源标识不是字符串或整数。
    IllegalScalar,
    /// 值对象或唯一身份规则拒绝，携带领域原文。
    Rejected(String),
}

impl std::fmt::Display for SnapshotIdentityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unregistered => formatter.write_str("该差异类型没有可注册的外部规范身份"),
            Self::NotJsonObject => formatter.write_str("规范化快照不是可验证的 JSON 对象"),
            Self::IllegalScalar => formatter.write_str("快照来源标识必须是字符串或整数"),
            Self::Rejected(message) => formatter.write_str(message),
        }
    }
}

impl From<SnapshotIdentityError> for Error {
    fn from(error: SnapshotIdentityError) -> Self {
        Self::from(error.to_string())
    }
}

impl MappingSourceIdentity {
    /// 从注册来源字段对应的 JSON 标量构造候选值。
    ///
    /// # 参数
    /// * `value` - 注册来源字段对应的 JSON 值
    ///
    /// # 返回
    /// 返回规范化来源身份候选值。
    ///
    /// # 错误
    /// 值不是字符串或整数时返回 `IllegalScalar`；
    /// 规范化后为空时返回携带领域原文的 `Rejected`。
    ///
    /// # 约束
    /// 纯值转换，不访问数据库；浮点、布尔、空值与复合类型一律拒绝。
    pub fn from_json_value(value: &Value) -> std::result::Result<Self, SnapshotIdentityError> {
        let text = match value {
            Value::String(value) => value.clone(),
            Value::Number(value) if value.is_i64() || value.is_u64() => value.to_string(),
            _ => return Err(SnapshotIdentityError::IllegalScalar),
        };
        Self::new(text).map_err(|error| SnapshotIdentityError::Rejected(error.to_string()))
    }
}

impl MappingTaskType {
    /// 从规范化快照 JSON 解析当前映射类型的唯一外部身份。
    ///
    /// # 参数
    /// * `snapshot` - 规范化商城快照 JSON 文本
    ///
    /// # 返回
    /// 返回由领域注册表判定的唯一来源身份。
    ///
    /// # 错误
    /// 映射类型未注册、快照不是 JSON 对象、来源字段类型非法，
    /// 或唯一身份规则不满足时返回对应的强类型失配。
    ///
    /// # 约束
    /// 纯值解析，不访问数据库；历史规范化快照的字段优先级保持不变。
    pub fn snapshot_external_identity(
        self,
        snapshot: &str,
    ) -> std::result::Result<MappingSourceIdentity, SnapshotIdentityError> {
        let registration = self
            .target_registration()
            .ok_or(SnapshotIdentityError::Unregistered)?;
        let value: Value =
            serde_json::from_str(snapshot).map_err(|_| SnapshotIdentityError::NotJsonObject)?;
        let object = value.as_object().ok_or(SnapshotIdentityError::NotJsonObject)?;
        let candidates = registration
            .source_identity_fields
            .iter()
            .filter_map(|field| object.get(*field))
            .map(MappingSourceIdentity::from_json_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        self.external_identity(&candidates)
            .map_err(|error| SnapshotIdentityError::Rejected(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{MappingSourceIdentity, MappingTaskType, SnapshotIdentityError};
    use serde_json::json;

    #[test]
    fn json_scalar_adapts_string_and_integer_only() {
        assert_eq!(
            MappingSourceIdentity::from_json_value(&json!(" C-1 "))
                .unwrap()
                .as_str(),
            "C-1"
        );
        assert_eq!(
            MappingSourceIdentity::from_json_value(&json!(42))
                .unwrap()
                .as_str(),
            "42"
        );
        assert_eq!(
            MappingSourceIdentity::from_json_value(&json!("   ")),
            Err(SnapshotIdentityError::Rejected(
                "快照来源标识不能为空".to_string()
            ))
        );
        for illegal in [
            json!(1.5),
            json!(true),
            json!(null),
            json!(["C-1"]),
            json!({"id": "C-1"}),
        ] {
            assert_eq!(
                MappingSourceIdentity::from_json_value(&illegal),
                Err(SnapshotIdentityError::IllegalScalar)
            );
        }
    }

    #[test]
    fn snapshot_identity_resolves_single_candidate_per_type() {
        assert_eq!(
            MappingTaskType::Customer
                .snapshot_external_identity(r#"{"customer_external_id":" C-1 "}"#)
                .unwrap()
                .as_str(),
            "C-1"
        );
        assert_eq!(
            MappingTaskType::Contract
                .snapshot_external_identity(r#"{"contract_id":42}"#)
                .unwrap()
                .as_str(),
            "42"
        );
    }

    #[test]
    fn snapshot_identity_requires_consistent_candidates() {
        let consistent = MappingTaskType::Customer
            .snapshot_external_identity(r#"{"customer_external_id":"C-1","customer_id":"C-1"}"#)
            .unwrap();
        assert_eq!(consistent.as_str(), "C-1");
        assert!(matches!(
            MappingTaskType::Contract
                .snapshot_external_identity(r#"{"contract_external_id":"CT-1","contract_id":"CT-2"}"#),
            Err(SnapshotIdentityError::Rejected(_))
        ));
        assert!(matches!(
            MappingTaskType::Customer.snapshot_external_identity(r#"{"other_field":"C-1"}"#),
            Err(SnapshotIdentityError::Rejected(_))
        ));
    }

    #[test]
    fn snapshot_identity_rejects_non_object_and_unregistered_type() {
        for invalid in [r#"["C-1"]"#, r#""C-1""#, "42", "{不是 json"] {
            assert_eq!(
                MappingTaskType::Customer.snapshot_external_identity(invalid),
                Err(SnapshotIdentityError::NotJsonObject)
            );
        }
        assert_eq!(
            MappingTaskType::UniqueLineItem.snapshot_external_identity(r#"{"line_id":"L-1"}"#),
            Err(SnapshotIdentityError::Unregistered)
        );
        assert_eq!(
            MappingTaskType::AmountFormat.snapshot_external_identity(r#"{"line_id":"L-1"}"#),
            Err(SnapshotIdentityError::Unregistered)
        );
    }
}
