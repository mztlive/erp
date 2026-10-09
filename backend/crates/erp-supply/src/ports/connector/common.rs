//! 来源证据、分页和失败语义；不替代正式业务实体或集成任务。

use std::cmp::Ordering;
use std::num::NonZeroU32;
use std::time::Duration;

use erp_core::common::time::Instant;
use thiserror::Error;

use crate::entity::failure::SupplierFailureClass;

/// 适配器结果；摘要必须脱敏，不承载原始 HTTP 正文。
pub type ConnectorResult<T> = Result<T, ConnectorError>;

/// 复用既有失败分类；请求可能已产生外部副作用时必须为 ResultUnknown。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("supplier connector {code}: {summary}")]
pub struct ConnectorError {
    pub class: SupplierFailureClass,
    pub code: String,
    pub summary: String,
    /// 对方建议的最早重试间隔；仅供调度参考，不构成安全重放许可。
    pub retry_after: Option<Duration>,
}

/// 外部商品身份。订货规格号必须非空；禁止按名称或条码自动绑定公司 SKU。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SupplierSku {
    pub product_id: Option<String>,
    pub spec_id: String,
}

/// 来源版本的比较能力；令牌只能判等，不能按字典序或接收次序排序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRevision {
    /// 相同连接、对象、字段组及 scope 内才允许比较序号。
    Sequence {
        scope: String,
        value: u64,
    },
    EqualityToken(String),
    Unversioned,
}

impl SourceRevision {
    /// 比较同一连接、对象及字段组的来源版本；调用方必须先核对这三个边界。
    ///
    /// # 参数
    /// `previous` 为同一比较边界内已保存的来源版本。
    /// # 返回
    /// 可证明的新旧/相等关系；无版本、不同 scope 或不等令牌返回 None，必须补查或转人工。
    /// # 错误
    /// 不返回错误；None 不得解释为更新。
    pub fn compare(&self, previous: &Self) -> Option<Ordering> {
        match (self, previous) {
            (Self::Sequence { scope, value }, Self::Sequence { scope: old_scope, value: old })
                if scope == old_scope =>
            {
                Some(value.cmp(old))
            },
            (Self::EqualityToken(token), Self::EqualityToken(old)) if token == old => Some(Ordering::Equal),
            _ => None,
        }
    }
}

/// 每份查询结果的来源证据；来源时间缺失时必须保留 None。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStamp {
    pub revision: SourceRevision,
    /// 供应商明确声明的业务变更时间，不能用签名时间戳代替。
    pub changed_at: Option<Instant>,
    /// 本次成功读取时间；不能用来排序供应商历史事件或自动延长其缓存有效期。
    pub observed_at: Instant,
}

/// 查询取得的快照。事实生效、来源可信度和有效期由消费方校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot<T> {
    pub stamp: SourceStamp,
    pub value: T,
}

/// 查询结果不存在不等于原写入绝未执行；不提供“查不到即可重放”捷径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup<T> {
    Found(T),
    NotVisible,
}

/// 固定扫描窗口。Changes 的上下界为包含关系；跨窗口重叠由消费方去重。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanWindow {
    Full,
    Changes { since: Instant, through: Instant },
}

/// 适配器封装供应商页号/游标；恢复时连接、窗口和筛选条件必须保持一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    pub window: ScanWindow,
    pub cursor: Option<String>,
    pub limit: NonZeroU32,
}

/// 只有当前页及其处理意图可靠保存后才推进 next；空页有 next 时仍须继续。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// None 只表示扫描结束，不表示源端提供一致快照或缺失对象已删除。
    pub next: Option<String>,
}

/// 调用方持久化的写动作身份；每个创建/支付/取消/退款步骤使用独立动作 ID。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionKey {
    /// 重试不得更换；适配器必须明确映射到供应商支持的幂等字段。
    pub id: String,
    /// 对规范化完整业务载荷的指纹；调用方拒绝同键异载荷，不记录个人资料明文。
    pub payload_hash: String,
}

/// 外部系统的写保护保证，不能由 ERP 自己生成幂等键冒充。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayProtection {
    /// 仅在供应商确认的有效窗口及相同请求载荷内成立，窗口外必须先调查。
    ProviderDeduplicated { retention: Duration },
    /// 未取得可靠幂等保证；禁止自动重放可能已送达的写请求。
    Unverified,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_comparison_rejects_old_observations_and_detects_duplicates() {
        let previous = SourceRevision::Sequence { scope: "stock-v1".into(), value: 10 };
        for (value, expected) in [(9, Ordering::Less), (10, Ordering::Equal), (11, Ordering::Greater)] {
            let incoming = SourceRevision::Sequence { scope: "stock-v1".into(), value };
            assert_eq!(incoming.compare(&previous), Some(expected));
        }
    }

    #[test]
    fn incomparable_versions_never_claim_to_be_newer() {
        let sequence = SourceRevision::Sequence { scope: "epoch-1".into(), value: 10 };
        let restarted = SourceRevision::Sequence { scope: "epoch-2".into(), value: 11 };
        assert_eq!(restarted.compare(&sequence), None);
        let token = SourceRevision::EqualityToken("v10".into());
        assert_eq!(token.compare(&SourceRevision::EqualityToken("v9".into())), None);
        assert_eq!(token.compare(&token), Some(Ordering::Equal));
        assert_eq!(SourceRevision::Unversioned.compare(&SourceRevision::Unversioned), None);
        assert_eq!(token.compare(&sequence), None);
    }
}
