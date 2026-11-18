//! 合同合法单据参与 Port：只查询本次授权目标的历史参与身份。

use std::sync::Arc;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 合同域消费的合法参与事实；adapter 读取工作流参与记录。
#[async_trait]
pub trait ContractParticipantPort: Send + Sync {
    /// 读取本次合同目标中的历史参与身份，不枚举账号的其他单据。
    /// 返回目标集合中去重后的参与单据 ID；空目标返回空集合。
    /// `user_id` 为当前账号，`ids` 为当前授权所检查的合同身份，`executor` 为与授权相同的执行器。
    /// 未装配或读取失败时拒绝。
    /// 历史参与必须来自有效业务参与事实，不得由签约经办或业绩快照推导。
    async fn document_ids_in(
        &self,
        user_id: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 未接线时失败关闭，不得把参与集合解释为空成功。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedContractParticipantPort;

impl FailClosedContractParticipantPort {
    /// 返回未接线的共享端口。
    /// 返回可注入合同 Service 的失败关闭端口。
    /// 读取动作补充历史参与前必须注入真实 adapter。
    pub fn shared() -> Arc<dyn ContractParticipantPort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl ContractParticipantPort for FailClosedContractParticipantPort {
    async fn document_ids_in(
        &self,
        _user_id: &str,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(Error::Internal("合同参与端口未接线".into()))
    }
}

/// 空参与集合，仅供不涉及历史参与的单元测试。
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyContractParticipants;

#[async_trait]
impl ContractParticipantPort for EmptyContractParticipants {
    async fn document_ids_in(
        &self,
        _user_id: &str,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::NoTransaction;
    use tokio::runtime::Builder;

    use super::*;

    #[test]
    fn fail_closed_participant_port_does_not_become_empty_success() {
        let runtime = Builder::new_current_thread().build().expect("runtime");
        runtime.block_on(async {
            let err = FailClosedContractParticipantPort
                .document_ids_in("user-1", &["contract-1".into()], &mut NoTransaction)
                .await
                .expect_err("must fail closed");
            match err {
                Error::Internal(message) => assert_eq!(message, "合同参与端口未接线"),
                other => panic!("期望 Internal，得到 {other:?}"),
            }
        });
    }
}
