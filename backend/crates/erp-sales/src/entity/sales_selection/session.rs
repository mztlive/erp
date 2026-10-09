//! 选品会话：不编号、不进经营账本。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{
    SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionProposalId, SalesSelectionSessionId,
};
use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::limits::{QUANTITY_MAX, QUANTITY_MIN};
use super::recipient::SelectionRecipient;
use super::types::SubmitMode;

/// 会话内一项选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChoice {
    /// 本册陈列项身份。
    pub display_item_id: SalesSelectionDisplayItemId,
    /// 按份采购的份数；商城兑换必须为空。
    pub quantity: Option<u32>,
}

/// 选品会话。
#[derive(Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesSelectionSession {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属选品册。
    pub booklet_id: SalesSelectionBookletId,
    /// 参与人稳定身份；普通模式和历史会话为空。
    #[serde(default)]
    pub participant_id: String,
    /// 提货券代码哈希。
    #[serde(default)]
    pub voucher_code_hash: Option<String>,
    /// 提货券代码密文。
    #[serde(default)]
    pub voucher_code_ciphertext: Option<String>,
    /// 个人提交生成的销售方案。
    #[serde(default)]
    pub proposal_id: Option<SalesSelectionProposalId>,
    /// 提交时冻结的个人收件信息。
    #[serde(default)]
    pub recipient: Option<SelectionRecipient>,
    /// 会话版本，成功保存后递增。
    pub session_version: u64,
    /// 当前选择。
    pub choices: Vec<SessionChoice>,
    /// 提交后冻结。
    pub frozen: bool,
}

impl fmt::Debug for SalesSelectionSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SalesSelectionSession")
            .field("id", &self.base.id)
            .field("session_version", &self.session_version)
            .field("frozen", &self.frozen)
            .finish_non_exhaustive()
    }
}

impl SalesSelectionSession {
    /// 首次发布建立空会话。
    ///
    /// # 参数
    /// * `id` - 会话身份
    /// * `booklet_id` - 选品册
    ///
    /// # 返回
    /// 返回版本为 1 的空会话。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: SalesSelectionSessionId, booklet_id: SalesSelectionBookletId) -> Self {
        Self {
            base: BaseModel::new(id.to_string()),
            booklet_id,
            participant_id: String::new(),
            voucher_code_hash: None,
            voucher_code_ciphertext: None,
            proposal_id: None,
            recipient: None,
            session_version: 1,
            choices: Vec::new(),
            frozen: false,
        }
    }

    /// 为一张提货券建立独立空会话。
    ///
    /// # 参数
    /// * `id` - 会话身份
    /// * `booklet_id` - 所属选品册
    /// * `participant_id` - 参与人稳定身份
    /// * `voucher_code_hash` - 券码哈希
    /// * `voucher_code_ciphertext` - 券码密文
    ///
    /// # 返回
    /// 返回版本为 1 的个人会话。
    ///
    /// # 错误
    /// 参与人身份为空、超长或券码哈希、密文为空时拒绝。
    pub fn new_participant(
        id: SalesSelectionSessionId,
        booklet_id: SalesSelectionBookletId,
        participant_id: String,
        voucher_code_hash: String,
        voucher_code_ciphertext: String,
    ) -> Result<Self> {
        let participant_id =
            normalize_required_text(participant_id, "参与人身份不能为空", 64, "参与人身份过长")?;
        if voucher_code_hash.trim().is_empty() || voucher_code_ciphertext.trim().is_empty() {
            return Err(Error::from("提货券代码哈希与密文不能为空"));
        }
        let mut session = Self::new(id, booklet_id);
        session.participant_id = participant_id;
        session.voucher_code_hash = Some(voucher_code_hash);
        session.voucher_code_ciphertext = Some(voucher_code_ciphertext);
        Ok(session)
    }

    /// 保存完整选择集合。
    ///
    /// # 参数
    /// * `expected_version` - 预期会话版本
    /// * `choices` - 完整选择
    /// * `submit_mode` - 册的提交方式
    /// * `allowed_ids` - 本册允许选择的陈列项
    ///
    /// # 返回
    /// 成功后递增版本并替换选择。
    ///
    /// # 错误
    /// 版本冲突、冻结、重复项、非法份数或不属于本册的项使整次失败。
    pub fn save(
        &mut self,
        expected_version: u64,
        choices: Vec<SessionChoice>,
        submit_mode: SubmitMode,
        allowed_ids: &[SalesSelectionDisplayItemId],
    ) -> Result<()> {
        self.ensure_writable(expected_version)?;
        self.choices = normalize_choices(choices, submit_mode, allowed_ids)?;
        self.session_version = self.session_version.saturating_add(1);
        Ok(())
    }

    /// 提交前校验版本并冻结。
    ///
    /// # 参数
    /// * `expected_version` - 确认清单上的会话版本
    ///
    /// # 返回
    /// 冻结会话。
    ///
    /// # 错误
    /// 版本冲突、已冻结或未选任何项时拒绝。
    pub fn freeze_for_submit(&mut self, expected_version: u64) -> Result<()> {
        self.ensure_writable(expected_version)?;
        if self.choices.is_empty() {
            return Err(Error::from("未选任何陈列项，不能提交"));
        }
        self.frozen = true;
        Ok(())
    }

    /// 按份采购重算金额合计。
    ///
    /// # 参数
    /// * `submit_mode` - 提交方式
    /// * `price_of` - 由陈列项身份取售价
    ///
    /// # 返回
    /// 商城兑换返回 `None`；按份采购返回合计。
    ///
    /// # 错误
    /// 缺份数、缺售价或溢出时拒绝。
    pub fn quantity_total<F>(&self, submit_mode: SubmitMode, mut price_of: F) -> Result<Option<Amount>>
    where
        F: FnMut(&SalesSelectionDisplayItemId) -> Option<Amount>,
    {
        if !submit_mode.requires_quantity() {
            return Ok(None);
        }
        let mut total = Amount::zero();
        for choice in &self.choices {
            let quantity = choice.quantity.ok_or_else(|| Error::from("按份采购必须填写份数"))?;
            let price = price_of(&choice.display_item_id).ok_or_else(|| Error::from("选择了无效陈列项"))?;
            total = super::pricing::try_add(total, super::pricing::try_mul_u32(price, quantity)?)?;
        }
        Ok(Some(total))
    }

    /// 校验可写且版本匹配。
    ///
    /// # 参数
    /// * `expected_version` - 预期版本
    ///
    /// # 返回
    /// 通过时允许写入。
    ///
    /// # 错误
    /// 已冻结或版本冲突时拒绝。
    fn ensure_writable(&self, expected_version: u64) -> Result<()> {
        if self.frozen {
            return Err(Error::from("选品已提交，不能再修改"));
        }
        if self.session_version != expected_version {
            return Err(Error::from("选品内容已被其他人更新，请核对后再提交"));
        }
        Ok(())
    }
}

/// 规范化完整选择集合。
///
/// # 参数
/// * `choices` - 原始选择
/// * `submit_mode` - 提交方式
/// * `allowed_ids` - 允许的陈列项
///
/// # 返回
/// 返回通过校验的选择。重复项整次拒绝，不会合并。
///
/// # 错误
/// 重复、越权、份数非法或混用时整次失败。
pub fn normalize_choices(
    choices: Vec<SessionChoice>,
    submit_mode: SubmitMode,
    allowed_ids: &[SalesSelectionDisplayItemId],
) -> Result<Vec<SessionChoice>> {
    let allowed: std::collections::BTreeSet<&str> = allowed_ids.iter().map(|id| id.as_ref()).collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::with_capacity(choices.len());
    for choice in choices {
        let id = choice.display_item_id.as_ref();
        if !allowed.contains(id) {
            return Err(Error::from("只能选择本选品册中的商品"));
        }
        if !seen.insert(id.to_string()) {
            return Err(Error::from("同一陈列项不能重复选择"));
        }
        normalized.push(SessionChoice {
            display_item_id: choice.display_item_id,
            quantity: normalize_quantity(choice.quantity, submit_mode)?,
        });
    }
    Ok(normalized)
}

/// 按提交方式校验份数。
///
/// # 参数
/// * `quantity` - 请求份数
/// * `submit_mode` - 提交方式
///
/// # 返回
/// 按份采购返回 1–100000；商城兑换返回 `None`。
///
/// # 错误
/// 缺份数、携带份数或超出范围时拒绝。
fn normalize_quantity(quantity: Option<u32>, submit_mode: SubmitMode) -> Result<Option<u32>> {
    match submit_mode {
        SubmitMode::ByQuantity | SubmitMode::PickupVoucher => {
            let quantity = quantity.ok_or_else(|| Error::from("按份采购必须填写份数"))?;
            if !(QUANTITY_MIN..=QUANTITY_MAX).contains(&quantity) {
                return Err(Error::from("份数必须是 1 到 100000 的整数"));
            }
            Ok(Some(quantity))
        },
        SubmitMode::MallRedeem => {
            if quantity.is_some() {
                return Err(Error::from("商城兑换不能填写份数"));
            }
            Ok(None)
        },
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionSessionId};

    use super::{SalesSelectionSession, SessionChoice, normalize_choices};
    use crate::entity::sales_selection::types::SubmitMode;

    #[test]
    fn mall_redeem_rejects_quantity() {
        let id = SalesSelectionDisplayItemId::new("d1");
        let error = normalize_choices(
            vec![SessionChoice { display_item_id: id.clone(), quantity: Some(1) }],
            SubmitMode::MallRedeem,
            &[id],
        )
        .unwrap_err();
        assert!(error.to_string().contains("不能填写份数"));
    }

    #[test]
    fn by_quantity_rejects_duplicate() {
        let id = SalesSelectionDisplayItemId::new("d1");
        let choice = SessionChoice { display_item_id: id.clone(), quantity: Some(2) };
        let error =
            normalize_choices(vec![choice.clone(), choice], SubmitMode::ByQuantity, &[id]).unwrap_err();
        assert!(error.to_string().contains("重复"));
    }
    #[test]
    fn voucher_sessions_are_independent_and_require_quantity() {
        let booklet_id = SalesSelectionBookletId::new("book-1");
        let mut first = SalesSelectionSession::new_participant(
            SalesSelectionSessionId::new("s1"),
            booklet_id.clone(),
            "person-1".into(),
            "hash-1".into(),
            "cipher-1".into(),
        )
        .unwrap();
        let second = SalesSelectionSession::new_participant(
            SalesSelectionSessionId::new("s2"),
            booklet_id,
            "person-2".into(),
            "hash-2".into(),
            "cipher-2".into(),
        )
        .unwrap();
        let item = SalesSelectionDisplayItemId::new("d1");
        first
            .save(
                1,
                vec![SessionChoice { display_item_id: item.clone(), quantity: Some(2) }],
                SubmitMode::PickupVoucher,
                std::slice::from_ref(&item),
            )
            .unwrap();
        assert_eq!(first.session_version, 2);
        assert_eq!(second.session_version, 1);
        assert!(second.choices.is_empty());
        assert!(
            normalize_choices(
                vec![SessionChoice { display_item_id: item.clone(), quantity: None }],
                SubmitMode::PickupVoucher,
                std::slice::from_ref(&item)
            )
            .is_err()
        );
        assert!(
            normalize_choices(
                vec![SessionChoice { display_item_id: item.clone(), quantity: Some(0) }],
                SubmitMode::PickupVoucher,
                &[item]
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_session_defaults_participant_and_personal_fields() {
        let session = SalesSelectionSession::new(
            SalesSelectionSessionId::new("s1"),
            SalesSelectionBookletId::new("book-1"),
        );
        let mut json = serde_json::to_value(session).unwrap();
        let object = json.as_object_mut().unwrap();
        for field in
            ["participant_id", "voucher_code_hash", "voucher_code_ciphertext", "proposal_id", "recipient"]
        {
            object.remove(field);
        }
        let restored: SalesSelectionSession = serde_json::from_value(json).unwrap();
        assert!(restored.participant_id.is_empty());
        assert!(restored.voucher_code_hash.is_none());
        assert!(restored.voucher_code_ciphertext.is_none());
        assert!(restored.proposal_id.is_none());
        assert!(restored.recipient.is_none());
    }
    #[test]
    fn session_debug_omits_voucher_credentials() {
        let session = SalesSelectionSession::new_participant(
            SalesSelectionSessionId::new("s1"),
            SalesSelectionBookletId::new("b1"),
            "person-1".into(),
            "secret-voucher-hash".into(),
            "secret-voucher-cipher".into(),
        )
        .unwrap();
        let debug = format!("{session:?}");
        assert!(debug.contains("s1"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("person-1"));
    }
}
