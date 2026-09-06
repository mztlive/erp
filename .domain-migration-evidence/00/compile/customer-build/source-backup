//! 客户资料输入的协议适配与结构校验。

use entities::customer::{
    CustomerProfileFactInput, CustomerProfileFactKind, CustomerProfileFactSet, CustomerProfileOperation,
    CustomerProfileReplayContext, CustomerProfileRequestFingerprint, CustomerProfileRequestShape,
};
use validator::Validate;

use crate::errors::Result;

use super::super::{
    CustomerProfileAddressInput, CustomerProfileBankAccountInput, CustomerProfileContactInput,
    SaveCustomerProfileRequest,
};

impl CustomerProfileFactInput for CustomerProfileContactInput {
    fn existing_id(&self) -> Option<&str> {
        self.existing_id.as_deref()
    }

    fn is_default(&self) -> bool {
        self.is_default
    }

    fn required_value(&self) -> Option<&str> {
        self.mobile.as_deref()
    }
}

impl CustomerProfileFactInput for CustomerProfileAddressInput {
    fn existing_id(&self) -> Option<&str> {
        self.existing_id.as_deref()
    }

    fn is_default(&self) -> bool {
        self.is_default
    }

    fn required_value(&self) -> Option<&str> {
        self.address.as_deref()
    }
}

impl CustomerProfileFactInput for CustomerProfileBankAccountInput {
    fn existing_id(&self) -> Option<&str> {
        self.existing_id.as_deref()
    }

    fn is_default(&self) -> bool {
        self.is_default
    }

    fn required_value(&self) -> Option<&str> {
        self.account_number.as_deref()
    }
}

impl SaveCustomerProfileRequest {
    /// 形成客户资料命令的稳定幂等重放上下文。
    ///
    /// 指纹基于当前 DTO 的 JSON 字节并带算法版本；原始联系人、地址和银行
    /// 账户正文只参与摘要计算，不进入重放上下文或命令记录。
    ///
    /// # 参数
    /// * `operation` - 创建或修订操作
    /// * `customer_id` - 修订目标客户；创建时为 `None`
    /// * `initiated_by` - 当前认证账号 ID
    ///
    /// # 返回
    /// 返回用于幂等查询、命令构造和事务失败恢复的稳定上下文。
    ///
    /// # 错误
    /// 请求无法序列化或上下文身份非法时返回错误。
    pub(super) fn replay_context(
        &self,
        operation: CustomerProfileOperation,
        customer_id: Option<&str>,
        initiated_by: &str,
    ) -> Result<CustomerProfileReplayContext> {
        let payload = serde_json::to_vec(self)
            .map_err(|_| crate::errors::Error::Internal("客户资料请求指纹计算失败".to_string()))?;
        let fingerprint = CustomerProfileRequestFingerprint::from_json_bytes_v1(&payload);
        Ok(CustomerProfileReplayContext::new(
            self.idempotency_key.clone(),
            operation,
            customer_id.map(str::to_string),
            initiated_by.to_string(),
            fingerprint,
        )?)
    }

    /// 校验 DTO 字段格式与嵌套输入协议。
    ///
    /// 只保留 `validator` 注解表达的协议校验；默认项、既有 ID、版本与
    /// 创建/修订结构组合由 entities 值对象校验。
    ///
    /// # 返回
    /// DTO 协议合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 根请求或任一嵌套输入违反 DTO 注解时返回校验错误。
    pub(super) fn validate_protocol(&self) -> Result<()> {
        self.validate()?;
        for contact in self.contacts.as_deref().unwrap_or_default() {
            contact.validate()?;
        }
        for address in self.addresses.as_deref().unwrap_or_default() {
            address.validate()?;
        }
        for account in self.bank_accounts.as_deref().unwrap_or_default() {
            account.validate()?;
        }
        Ok(())
    }

    /// 将 DTO 字段适配为 entities 客户资料结构值对象并执行领域校验。
    ///
    /// # 参数
    /// * `operation` - 创建或修订操作
    ///
    /// # 返回
    /// 纯结构规则全部满足时返回 `Ok(())`。
    ///
    /// # 错误
    /// 版本/负责人组合、默认项、既有 ID 或新增事实必填值非法时返回错误。
    pub(super) fn validate_structure(&self, operation: CustomerProfileOperation) -> Result<()> {
        let shape = CustomerProfileRequestShape::new(
            operation,
            self.expected_party_version,
            self.expected_customer_version,
            self.owner_user_id.is_some(),
        )?;
        CustomerProfileFactSet::new(CustomerProfileFactKind::Contact, self.contacts.as_deref())
            .validate(shape.operation())?;
        CustomerProfileFactSet::new(CustomerProfileFactKind::Address, self.addresses.as_deref())
            .validate(shape.operation())?;
        CustomerProfileFactSet::new(
            CustomerProfileFactKind::BankAccount,
            self.bank_accounts.as_deref(),
        )
        .validate(shape.operation())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entities::customer::CustomerProfileOperation;
    use serde_json::json;

    use super::SaveCustomerProfileRequest;

    #[test]
    fn request_builds_stable_versioned_context_without_retaining_sensitive_body() {
        let request: SaveCustomerProfileRequest = serde_json::from_value(json!({
            "idempotency_key": " save-customer-1 ",
            "legal_name": "示例客户",
            "contacts": [{
                "contact_name": "张三",
                "mobile": "13800000000",
                "is_default": true
            }],
            "bank_accounts": [{
                "account_name": "示例客户",
                "bank_name": "示例银行",
                "account_number": "6222020000000000",
                "is_default": true
            }],
            "effective_from": "2026-08-31",
            "change_reason": "首次建档"
        }))
        .unwrap();

        let first = request
            .replay_context(CustomerProfileOperation::Create, None, "admin-1")
            .unwrap();
        let second = request
            .replay_context(CustomerProfileOperation::Create, None, "admin-1")
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first.request_fingerprint().as_str(),
            "sha256-json-v1:07525242bd0a4ce006b4ec1bd7b60c88d2fcf009ca693e43a0a403a82922f8b7"
        );
        assert_eq!(first.idempotency_key(), "save-customer-1");
        assert!(first
            .request_fingerprint()
            .as_str()
            .starts_with("sha256-json-v1:"));
        let debug_context = format!("{first:?}");
        assert!(!debug_context.contains("13800000000"));
        assert!(!debug_context.contains("6222020000000000"));
    }

    #[test]
    fn request_fingerprint_changes_when_sensitive_payload_changes() {
        let request: SaveCustomerProfileRequest =
            serde_json::from_value(base_request("13800000000")).unwrap();
        let changed: SaveCustomerProfileRequest =
            serde_json::from_value(base_request("13900000000")).unwrap();
        let first = request
            .replay_context(CustomerProfileOperation::Create, None, "admin-1")
            .unwrap();
        let second = changed
            .replay_context(CustomerProfileOperation::Create, None, "admin-1")
            .unwrap();
        assert_ne!(first.request_fingerprint(), second.request_fingerprint());
    }

    fn base_request(mobile: &str) -> serde_json::Value {
        json!({
            "idempotency_key": "save-customer-1",
            "legal_name": "示例客户",
            "contacts": [{
                "contact_name": "张三",
                "mobile": mobile,
                "is_default": true
            }],
            "effective_from": "2026-08-31",
            "change_reason": "首次建档"
        })
    }
}
