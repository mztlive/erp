//! `ReceivableFundsReviewChain` 卡券票款复核链值对象（FIN-E12）。
//!
//! 从有序复核事实验证连续性、解析链尾、生成下一复核号、校验前驱并计算
//! 确定性 version/hash。链不合法时禁止生成 next review。读取排序、唯一冲突
//! 与事务重试仍由 Repository／Service 负责。

use sha2::{Digest, Sha256};

use super::ReceivableFundsReview;
use crate::errors::{Error, Result};

const CHAIN_HASH_PREFIX: &str = "receivable-review-chain-v1";

/// 已验证连续性的应收复核链。
///
/// 构造时按 `review_no`、主键排序并校验：空链合法；非空链必须从 1 连续递增、
/// 首条无前驱、其后 `supersedes_review_id` 等于前一条主键。无序但可还原的输入
/// 与有序输入产生同一链与同一 hash。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivableFundsReviewChain {
    reviews: Vec<ReceivableFundsReview>,
    version: String,
}

impl ReceivableFundsReviewChain {
    /// 从复核事实构造已验证复核链。
    ///
    /// # 参数
    /// * `reviews` - 同一应收账户的复核事实（允许无序）
    ///
    /// # 返回
    /// 返回连续性已验证、hash 已冻结的链。
    ///
    /// # 错误
    /// 编号缺口、重复编号、错误前驱或重复主键时返回
    /// [`Error::LogicError`]（`应收复核链连续性损坏`），不产生部分链。
    ///
    /// # 约束
    /// 不读取数据库，不生成下一复核号以外的身份；溢出在 [`Self::next_review_no`]
    /// 单独失败。
    pub fn from_reviews(reviews: &[ReceivableFundsReview]) -> Result<Self> {
        let mut ordered = reviews.to_vec();
        ordered.sort_by(|left, right| {
            left.review_no
                .cmp(&right.review_no)
                .then_with(|| left.base.id.cmp(&right.base.id))
        });
        let mut seen_ids = std::collections::HashSet::new();
        for (index, review) in ordered.iter().enumerate() {
            let expected_no = u32::try_from(index)
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| Error::from("应收复核链连续性损坏"))?;
            let expected_predecessor = index
                .checked_sub(1)
                .and_then(|previous| ordered.get(previous))
                .map(|previous| previous.base.id.as_str());
            if review.review_no != expected_no
                || review.supersedes_review_id.as_ref().map(AsRef::as_ref) != expected_predecessor
                || !seen_ids.insert(review.base.id.clone())
            {
                return Err(Error::from("应收复核链连续性损坏"));
            }
        }
        let version = chain_version(&ordered);
        Ok(Self {
            reviews: ordered,
            version,
        })
    }

    /// 返回按复核号排序的链内事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已验证切片，空链为空。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 调用方不得绕过构造修改顺序。
    pub fn reviews(&self) -> &[ReceivableFundsReview] {
        &self.reviews
    }

    /// 返回链尾复核主键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 空链返回 `None`，否则返回最后一条主键。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 链尾由构造时的连续性保证，不扫描全表。
    pub fn tail_id(&self) -> Option<&str> {
        self.reviews.last().map(|review| review.base.id.as_str())
    }

    /// 返回不可由客户端解释或递增的复核链版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `rcv:` 前缀的确定性 hex。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 相同有序事实必得相同字节；算法与原 Service helper 一致。
    pub fn version(&self) -> &str {
        &self.version
    }

    /// 生成链的下一连续复核号。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 空链返回 `1`，否则返回链尾 `review_no + 1`。
    ///
    /// # 错误
    /// 链尾复核号为 `u32::MAX` 时返回 [`Error::LogicError`]（`应收复核号已达到上限`）。
    ///
    /// # 约束
    /// 只允许在已验证链上调用；非法链无法构造，因此无法生成 next review。
    pub fn next_review_no(&self) -> Result<u32> {
        self.reviews.last().map_or(Ok(1), |review| {
            review
                .review_no
                .checked_add(1)
                .ok_or_else(|| Error::from("应收复核号已达到上限"))
        })
    }

    /// 校验新复核声明的前驱等于当前链尾。
    ///
    /// # 参数
    /// * `predecessor_id` - 新复核携带的 `supersedes_review_id`
    ///
    /// # 返回
    /// 与 [`Self::tail_id`] 一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 不一致时返回 [`Error::LogicError`]（`应收复核链连续性损坏`）。
    ///
    /// # 约束
    /// 不写入新复核；数据库唯一冲突仍由仓储检测。
    pub fn ensure_predecessor(&self, predecessor_id: Option<&str>) -> Result<()> {
        if self.tail_id() == predecessor_id {
            Ok(())
        } else {
            Err(Error::from("应收复核链连续性损坏"))
        }
    }

    /// 校验同步差额复核已建立在既有期初链上。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 链非空时返回 `Ok(())`。
    ///
    /// # 错误
    /// 空链返回 [`Error::LogicError`]（`同步差额复核必须建立在既有期初复核链上`）。
    ///
    /// # 约束
    /// 期初复核允许空链；本方法只在差额复核命令下由 Service 调用。
    pub fn ensure_sync_delta_allowed(&self) -> Result<()> {
        if self.reviews.is_empty() {
            return Err(Error::from("同步差额复核必须建立在既有期初复核链上"));
        }
        Ok(())
    }

    /// 测试专用：改写链尾复核号以覆盖 `next_review_no` 溢出。
    ///
    /// # 参数
    /// * `review_no` - 替换后的链尾复核号
    ///
    /// # 返回
    /// 返回仍持有原 hash 的链（hash 不重算，仅供溢出分支）。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 仅测试编译；生产路径不得改写已验证链。
    #[cfg(test)]
    fn with_tail_review_no(mut self, review_no: u32) -> Self {
        if let Some(tail) = self.reviews.last_mut() {
            tail.review_no = review_no;
        }
        self
    }
}

/// 按固定字段顺序计算复核链版本。
///
/// # 参数
/// * `reviews` - 已按复核号排序的事实
///
/// # 返回
/// 返回 `rcv:<hex>`。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 字段集合与长度前缀编码必须与原 `review_chain_version` 字节级一致。
fn chain_version(reviews: &[ReceivableFundsReview]) -> String {
    let mut digest = Sha256::new();
    digest_part(&mut digest, CHAIN_HASH_PREFIX);
    for review in reviews {
        digest_part(&mut digest, &review.base.id);
        digest_part(&mut digest, &review.review_no.to_string());
        digest_part(&mut digest, review.review_type.as_str());
        digest_part(&mut digest, review.work_item_id.as_ref());
        digest_part(
            &mut digest,
            review
                .evidence_document_id
                .as_ref()
                .map(AsRef::as_ref)
                .unwrap_or_default(),
        );
        digest_part(
            &mut digest,
            review.evidence_reference.as_deref().unwrap_or_default(),
        );
        digest_part(&mut digest, review.review_result.as_str());
        digest_part(&mut digest, &review.reviewed_by);
        digest_part(&mut digest, &review.reviewed_at.unix_secs().to_string());
        digest_part(
            &mut digest,
            review
                .supersedes_review_id
                .as_ref()
                .map(AsRef::as_ref)
                .unwrap_or_default(),
        );
    }
    format!("rcv:{}", hex::encode(digest.finalize()))
}

/// 向摘要写入无拼接歧义的长度前缀字段。
///
/// # 参数
/// * `digest` - SHA-256 累加器
/// * `value` - 字段文本
///
/// # 返回
/// 无。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 先写 big-endian 长度再写 UTF-8 字节，禁止直接拼接。
fn digest_part(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::ReceivableFundsReviewChain;
    use crate::common::time::Instant;
    use crate::ids::{FileAssetId, ReceivableAccountId, ReceivableFundsReviewId, WorkItemId};
    use crate::receivable::{
        FundsReviewType, ReceivableFundsReview, ReceivableFundsReviewData, ReviewResult,
    };

    fn review(
        id: &str,
        no: u32,
        predecessor: Option<&str>,
        work_item: &str,
        at: i64,
    ) -> ReceivableFundsReview {
        ReceivableFundsReview::new(
            ReceivableFundsReviewId::new(id),
            ReceivableFundsReviewData {
                receivable_account_id: ReceivableAccountId::new("ra-1"),
                review_no: no,
                review_type: if no == 1 {
                    FundsReviewType::Opening
                } else {
                    FundsReviewType::SyncDelta
                },
                work_item_id: WorkItemId::new(work_item),
                evidence_document_id: Some(FileAssetId::new("file-1")),
                evidence_reference: Some("BANK-1".to_string()),
                review_result: ReviewResult::Passed,
                reviewed_by: "alice".to_string(),
                reviewed_at: Instant::from_unix_secs(at),
                supersedes_review_id: predecessor.map(ReceivableFundsReviewId::new),
            },
        )
        .unwrap()
    }

    #[test]
    fn empty_chain_is_legal_and_starts_at_one() {
        let chain = ReceivableFundsReviewChain::from_reviews(&[]).unwrap();
        assert!(chain.reviews().is_empty());
        assert_eq!(chain.tail_id(), None);
        assert_eq!(chain.next_review_no().unwrap(), 1);
        assert!(chain.version().starts_with("rcv:"));
        assert_eq!(chain.version().len(), 4 + 64);
        chain.ensure_predecessor(None).unwrap();
        assert!(chain.ensure_predecessor(Some("r-1")).is_err());
    }

    #[test]
    fn legal_chain_exposes_tail_next_and_predecessor() {
        let first = review("r-1", 1, None, "wi-1", 1_700_000_000);
        let second = review("r-2", 2, Some("r-1"), "wi-2", 1_700_000_100);
        let chain = ReceivableFundsReviewChain::from_reviews(&[first, second]).unwrap();
        assert_eq!(chain.tail_id(), Some("r-2"));
        assert_eq!(chain.next_review_no().unwrap(), 3);
        chain.ensure_predecessor(Some("r-2")).unwrap();
        assert!(chain.ensure_predecessor(Some("r-1")).is_err());
        assert!(chain.ensure_predecessor(None).is_err());
    }

    #[test]
    fn numbering_gap_wrong_predecessor_and_duplicate_are_rejected() {
        let first = review("r-1", 1, None, "wi-1", 1_700_000_000);
        let gapped = review("r-3", 3, Some("r-1"), "wi-3", 1_700_000_200);
        assert_eq!(
            ReceivableFundsReviewChain::from_reviews(&[first.clone(), gapped])
                .unwrap_err()
                .to_string(),
            "应收复核链连续性损坏"
        );

        let wrong_pred = review("r-2", 2, Some("r-other"), "wi-2", 1_700_000_100);
        assert!(ReceivableFundsReviewChain::from_reviews(&[first.clone(), wrong_pred]).is_err());

        let duplicate_no = review("r-2", 1, None, "wi-2", 1_700_000_100);
        assert!(ReceivableFundsReviewChain::from_reviews(&[first.clone(), duplicate_no]).is_err());

        let duplicate_id = review("r-1", 2, Some("r-1"), "wi-2", 1_700_000_100);
        assert!(ReceivableFundsReviewChain::from_reviews(&[first, duplicate_id]).is_err());
    }

    #[test]
    fn overflow_forbids_next_review() {
        let tail = review("r-max", 1, None, "wi-1", 1_700_000_000);
        let chain = ReceivableFundsReviewChain::from_reviews(&[tail])
            .unwrap()
            .with_tail_review_no(u32::MAX);
        assert_eq!(
            chain.next_review_no().unwrap_err().to_string(),
            "应收复核号已达到上限"
        );
    }

    #[test]
    fn unordered_input_matches_sorted_hash() {
        let first = review("r-1", 1, None, "wi-1", 1_700_000_000);
        let second = review("r-2", 2, Some("r-1"), "wi-2", 1_700_000_100);
        let ordered = ReceivableFundsReviewChain::from_reviews(&[first.clone(), second.clone()]).unwrap();
        let unordered = ReceivableFundsReviewChain::from_reviews(&[second, first]).unwrap();
        assert_eq!(ordered.version(), unordered.version());
        assert_eq!(ordered.tail_id(), unordered.tail_id());
        assert_eq!(ordered.reviews()[0].base.id, "r-1");
    }

    #[test]
    fn hash_is_deterministic_and_sensitive_to_facts() {
        let first = review("r-1", 1, None, "wi-1", 1_700_000_000);
        let chain = ReceivableFundsReviewChain::from_reviews(std::slice::from_ref(&first)).unwrap();
        let again = ReceivableFundsReviewChain::from_reviews(std::slice::from_ref(&first)).unwrap();
        assert_eq!(chain.version(), again.version());
        let mut other = first;
        other.reviewed_by = "bob".to_string();
        let changed = ReceivableFundsReviewChain::from_reviews(&[other]).unwrap();
        assert_ne!(chain.version(), changed.version());
    }

    #[test]
    fn illegal_chain_cannot_be_constructed_to_mint_next_review() {
        let mut first = review("r-1", 1, None, "wi-1", 1_700_000_000);
        first.supersedes_review_id = Some(ReceivableFundsReviewId::new("ghost"));
        assert!(ReceivableFundsReviewChain::from_reviews(&[first]).is_err());
    }

    #[test]
    fn sync_delta_requires_non_empty_chain() {
        let empty = ReceivableFundsReviewChain::from_reviews(&[]).unwrap();
        assert_eq!(
            empty.ensure_sync_delta_allowed().unwrap_err().to_string(),
            "同步差额复核必须建立在既有期初复核链上"
        );
        let first = review("r-1", 1, None, "wi-1", 1_700_000_000);
        ReceivableFundsReviewChain::from_reviews(&[first])
            .unwrap()
            .ensure_sync_delta_allowed()
            .unwrap();
    }
}
