//! 资金业务筛选使用的组织事实端口；权限由 identity 的统一授权入口执行。
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::{Error, Result};

/// 仅提供组织展开事实，不执行或解释权限规则。
#[async_trait]
pub trait FundsDataScopePort: Send + Sync {
    /// 展开请求中的组织及其可选下级。
    /// 返回启用节点的组织 ID 集合。
    /// 失败条件：未知组织拒绝，不得忽略后查询全部组织。
    /// 业务边界：筛选只能收窄授权结果；不得把完整组织树交给资金域自行重建。
    ///
    /// 输入约定：
    /// - `org_unit_ids`：请求中的组织 ID
    /// - `include_descendants`：是否包含有效下级
    /// - `executor`：与授权相同的执行器
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>>;
}

/// 未装配时组织事实读取失败关闭。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFundsDataScopePort;
impl FailClosedFundsDataScopePort {
    /// 返回未装配的共享组织事实端口。
    /// 结果：所有事实读取均失败的端口。
    pub fn shared() -> Arc<dyn FundsDataScopePort> {
        Arc::new(Self)
    }
}
#[async_trait]
impl FundsDataScopePort for FailClosedFundsDataScopePort {
    async fn expand_org_units(
        &self,
        _org_unit_ids: &[String],
        _include_descendants: bool,
        _executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Err(unwired())
    }
}
fn unwired() -> Error {
    Error::Internal("资金组织事实端口未接线".into())
}

#[cfg(test)]
mod tests {
    use persistence_core::NoTransaction;

    use super::*;
    #[tokio::test]
    async fn unwired_organization_reads_fail_closed() {
        let port = FailClosedFundsDataScopePort;
        assert!(port.expand_org_units(&["org".into()], true, &mut NoTransaction).await.is_err());
    }
}
