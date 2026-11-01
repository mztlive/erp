//! 合同我方结算主体必须由启用公司身份证明，不得使用客户主体缺省。

use erp_core::ids::PartyId;
use persistence_core::Executor;

use crate::ports::SigningCompanyPort;
use crate::{Error, Result};

/// 在当前执行器内证明主体具有启用的我方公司身份。
///
/// # 参数
/// * `companies` - 公司事实端口。
/// * `id` - 请求明确选择的我方主体。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 启用公司及名称事实完整时成功。
///
/// # 错误
/// 主体不是公司、停用、名称缺失或读取失败时拒绝。
pub(super) async fn require_signing_company(
    companies: &dyn SigningCompanyPort,
    id: &PartyId,
    executor: &mut dyn Executor,
) -> Result<()> {
    let company = companies
        .company(id.as_ref(), executor)
        .await?
        .filter(|company| company.active && !company.name.trim().is_empty());
    if company.is_none() {
        return Err(Error::ValidationError("我方结算主体必须是启用且资料完整的我方公司".into()));
    }
    Ok(())
}

/// 要求归档请求明确指定我方结算主体。
///
/// # 参数
/// * `id` - 请求中的主体身份。
///
/// # 返回
/// 返回非空的明确主体身份。
///
/// # 错误
/// 请求省略主体或身份为空时拒绝，不从客户事实推断。
pub(super) fn required_settlement(id: Option<&PartyId>) -> Result<PartyId> {
    id.filter(|id| !id.as_ref().trim().is_empty())
        .cloned()
        .ok_or_else(|| Error::ValidationError("归档合同必须明确选择我方结算主体".into()))
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::ports::SigningCompanyFact;

    struct Companies(Option<(String, bool)>);
    #[async_trait]
    impl SigningCompanyPort for Companies {
        async fn company(&self, id: &str, _: &mut dyn Executor) -> Result<Option<SigningCompanyFact>> {
            assert_eq!(id, "our-company");
            Ok(self
                .0
                .as_ref()
                .map(|(name, active)| SigningCompanyFact { name: name.clone(), active: *active }))
        }
    }

    #[tokio::test]
    async fn requires_explicit_available_company_without_customer_fallback() {
        let id = PartyId::new("our-company");
        assert!(required_settlement(None).is_err());
        assert_eq!(required_settlement(Some(&id)).unwrap(), id);
        require_signing_company(&Companies(Some(("我方公司".into(), true))), &id, &mut NoTransaction)
            .await
            .unwrap();
        for facts in [None, Some(("我方公司".into(), false)), Some((" ".into(), true))] {
            assert!(require_signing_company(&Companies(facts), &id, &mut NoTransaction).await.is_err());
        }
    }
}
