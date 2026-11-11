//! 资金业务筛选使用的组织事实端口；权限由 identity 的统一授权入口执行。
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use crate::{Error, Result};

/// 仅提供组织展开与有效成员事实，不执行或解释权限规则。
#[async_trait]
pub trait FundsDataScopePort: Send + Sync {
    /// 展开请求中的组织及其可选下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织拒绝，不得忽略后查询全部组织。
    ///
    /// # 关键业务约束
    /// 筛选只能收窄授权结果；不得把完整组织树交给资金域自行重建。
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>>;

    /// 读取指定内部组织在给定时点的有效主属成员。
    ///
    /// # 参数
    /// * `org_unit_ids` - 已展开的内部组织
    /// * `at` - 与授权相同的解析时点
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回排序去重后的人员 ID。
    ///
    /// # 错误
    /// 组织关系非法或读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 过期或未生效成员不得进入当前负责人组织筛选。
    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 未装配时组织事实读取失败关闭。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFundsDataScopePort;
impl FailClosedFundsDataScopePort {
    /// 返回未装配的共享组织事实端口。
    /// # 参数
    /// 无。
    /// # 返回
    /// 所有事实读取均失败的端口。
    /// # 错误
    /// 不执行 I/O。
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

    async fn org_member_ids(
        &self,
        _org_unit_ids: &BTreeSet<String>,
        _at: Instant,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
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
        assert!(
            port.org_member_ids(
                &BTreeSet::from(["org".into()]),
                Instant::from_unix_secs(1),
                &mut NoTransaction
            )
            .await
            .is_err()
        );
    }
}
