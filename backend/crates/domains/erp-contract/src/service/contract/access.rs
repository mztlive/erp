//! 合同对象按独立负责销售、明确共享及负责销售组织授权；客户关系只用于建档资格。

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
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
/// 将UTC授权时点转换为业务日期。
/// # 参数
/// at为授权时点。
/// # 返回
/// 上海业务日期。
/// # 错误
/// 日期无法表示时返回错误。
pub(super) fn business_date(at: Instant) -> Result<BusinessDate> {
    let date = at.as_utc() + chrono::Duration::hours(8);
    Ok(date.format("%Y-%m-%d").to_string().parse()?)
}

pub(super) fn scope_fingerprint_input(
    owned: &[String],
    collaborating: &[String],
    history: impl ScopeFingerprintHistory,
    org_owned: &[(Vec<String>, Vec<String>)],
) -> Vec<u8> {
    let mut out = Vec::new();
    push_str_section(&mut out, owned);
    push_str_section(&mut out, collaborating);
    history.push_section(&mut out);
    let mut orgs: Vec<(&[String], &[String])> =
        org_owned.iter().map(|(orgs, customers)| (orgs.as_slice(), customers.as_slice())).collect();
    orgs.sort();
    for (org_set, customers) in orgs {
        push_str_section(&mut out, org_set);
        push_str_section(&mut out, customers);
    }
    out
}

/// 范围指纹历史段的抽象：字符串列表或合同版本列表统一压入同一规范字节串。
pub(super) trait ScopeFingerprintHistory {
    /// 把历史段按规范编码追加到 `out`。
    ///
    /// # 参数
    /// * `out` - 累积中的规范字节串。
    ///
    /// # 返回
    /// 无返回值。排序后的历史段写入 `out`。
    ///
    /// # 错误
    /// 不返回错误。
    fn push_section(self, out: &mut Vec<u8>);
}

impl ScopeFingerprintHistory for &[String] {
    fn push_section(self, out: &mut Vec<u8>) {
        let mut sorted: Vec<&str> = self.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        push_str_section(out, &sorted);
    }
}

impl ScopeFingerprintHistory for &[crate::repository::scope::ContractVersion] {
    fn push_section(self, out: &mut Vec<u8>) {
        let mut pairs: Vec<(&str, u64)> =
            self.iter().map(|version| (version.id.as_str(), version.version)).collect();
        pairs.sort_unstable();
        push_u64(out, pairs.len() as u64);
        for (id, version) in pairs {
            push_bytes(out, id.as_bytes());
            push_u64(out, version);
        }
    }
}

fn push_str_section(out: &mut Vec<u8>, values: &[impl AsRef<str>]) {
    let mut sorted: Vec<&str> = values.iter().map(AsRef::as_ref).collect();
    sorted.sort_unstable();
    push_u64(out, sorted.len() as u64);
    for value in sorted {
        push_bytes(out, value.as_bytes());
    }
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
/// * `input` - `scope_fingerprint_input` 输出的规范字节串
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
