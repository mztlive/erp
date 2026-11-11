//! 合同对象按独立负责销售、明确共享及负责销售组织授权；客户关系只用于建档资格。

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::authorization::{CompiledScope, ContractAuthorization};
use crate::error::Result;

/// 合同对象访问范围；列表、详情、附件、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct ContractAccess {
    db: Database,
    scope: ContractAuthorization,
}

impl ContractAccess {
    /// 绑定合同集合与已装配完整事实模型的授权能力。
    ///
    /// # 参数
    /// * `db` - 合同集合所在数据库
    /// * `scope` - 组合层装配的固定授权能力
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围；不得用签约经办充当当前负责人。
    pub fn new(db: Database, scope: ContractAuthorization) -> Self {
        Self { db, scope }
    }

    /// 编译统一授权条件。
    /// # 参数
    /// actor、action为入口，executor为原执行器。
    /// # 返回
    /// identity凭证及查询载体。
    /// # 错误
    /// 授权或读取失败时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        self.scope.resolve(actor, action, executor).await
    }
    /// 在独立事务检查完整合同对象。
    /// # 参数
    /// actor、action、contract_id为可信操作及目标。
    /// # 返回
    /// 当前阶段元数据。
    /// # 错误
    /// 身份域拒绝或读取失败返回错误。
    pub async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        contract_id: &str,
    ) -> Result<CompiledScope> {
        let this = self.clone();
        let actor = actor.clone();
        let action = action.to_owned();
        let id = contract_id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_with(actor.clone(), &action, &id, executor).await?;
                    this.resolve(&actor, &action, executor).await
                })
            })
            .await
    }
    /// 在原事务内检查完整合同对象。
    /// # 参数
    /// actor、action、contract_id为可信入口及目标，executor为业务执行器。
    /// # 返回
    /// 完整对象授权通过时成功。
    /// # 错误
    /// 身份域拒绝时返回错误。
    pub async fn require_with(
        &self,
        actor: AuditActor,
        action: &str,
        contract_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.scope.require(&actor, action, contract_id, executor).await
    }
    /// 按实际客户来源检查创建资格。
    /// # 参数
    /// actor、customer_id为操作人与完整来源，executor为原执行器。
    /// # 返回
    /// identity凭证。
    /// # 错误
    /// 来源不存在或无资格拒绝。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        self.scope.require_create(actor, customer_id, executor).await
    }
}
/// 编码合同身份和业务版本；输入顺序不影响分页指纹。
pub(super) fn version_fingerprint_input(versions: &[crate::repository::scope::ContractVersion]) -> Vec<u8> {
    let mut pairs = versions.iter().map(|version| (version.id.as_str(), version.version)).collect::<Vec<_>>();
    pairs.sort_unstable();
    let mut out = Vec::new();
    push_u64(&mut out, u64::try_from(pairs.len()).expect("合同版本集合长度可表示为 u64"));
    for (id, version) in pairs {
        push_bytes(&mut out, id.as_bytes());
        push_u64(&mut out, version);
    }
    out
}

fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    push_u64(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// 对规范字节串计算稳定指纹（FNV-1a 64 位固定算法，不随工具链漂移）。
///
/// # 参数
/// * `input` - `version_fingerprint_input` 输出的规范字节串
///
/// # 返回
/// 返回 64 位指纹，仅用于同版本内快照比对。
///
/// # 错误
/// 不返回错误。
pub(super) fn stable_fingerprint(input: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::scope::ContractVersion;

    #[test]
    fn version_fingerprint_is_order_independent_and_tracks_business_changes() {
        let versions = [
            ContractVersion { id: "contract-b".into(), version: 2 },
            ContractVersion { id: "contract-a".into(), version: 1 },
        ];
        let reordered = [
            ContractVersion { id: "contract-a".into(), version: 1 },
            ContractVersion { id: "contract-b".into(), version: 2 },
        ];
        let changed = [
            ContractVersion { id: "contract-a".into(), version: 2 },
            ContractVersion { id: "contract-b".into(), version: 2 },
        ];
        let fingerprint = |rows: &[ContractVersion]| stable_fingerprint(&version_fingerprint_input(rows));
        assert_eq!(fingerprint(&versions), fingerprint(&reordered));
        assert_ne!(fingerprint(&versions), fingerprint(&changed));
        assert_ne!(fingerprint(&[]), fingerprint(&versions));
    }
}
