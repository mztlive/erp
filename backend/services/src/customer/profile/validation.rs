//! 客户资料输入的协议适配与结构校验。

use entities::customer::{
    CustomerProfileFactInput, CustomerProfileFactKind, CustomerProfileFactSet, CustomerProfileOperation,
    CustomerProfileRequestShape,
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
