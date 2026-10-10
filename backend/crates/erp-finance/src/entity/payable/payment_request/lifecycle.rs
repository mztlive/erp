//! 付款申请状态迁移，所有失败在修改实体之前完成校验。

use std::collections::HashSet;

use entity_core::BaseModel;
use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::types::{positive, required, sum};
use super::{
    PaymentRequestAllocation, PaymentRequestSubmission, PurchasePaymentRequest, PurchasePaymentRequestData,
    PurchasePaymentRequestStatus,
};

impl PurchasePaymentRequest {
    /// 创建不占用额度的申请草稿。
    ///
    /// # 参数
    /// * `id` - 服务端生成的申请身份。
    /// * `data` - 采购填写的表单。
    /// * `actor` - 已认证申请人。
    /// # 返回
    /// 返回未冻结来源且不占用应付的草稿。
    /// # 错误
    /// 表单或申请人无效、总金额溢出时返回错误。
    pub fn new(id: String, data: PurchasePaymentRequestData, actor: &str) -> Result<Self> {
        let data = data.normalize()?;
        let total_amount = sum(data.lines.iter().map(|line| line.amount))?;
        Ok(Self {
            request_no: format!("FKSQ-{id}"),
            base: BaseModel::new(id),
            status: PurchasePaymentRequestStatus::Draft,
            supplier_id: data.supplier_id.clone(),
            payer_party_id: String::new(),
            payer_party_name: String::new(),
            payee_account: None,
            org_unit_id: None,
            created_by: required(actor.to_owned(), "申请人", 128)?,
            data,
            lines: Vec::new(),
            total_amount,
            submission_version: 0,
            approval_subject_version: 0,
            approval_instance_id: None,
            work_item_id: None,
            last_reason: None,
        })
    }

    /// 修改草稿、撤回或驳回后的申请资料。
    ///
    /// # 参数
    /// * `data` - 新表单。
    /// # 返回
    /// 成功时替换表单并回到草稿，保留先前冻结材料至重新提交。
    /// # 错误
    /// 审批中或已批准申请不可修改，表单非法时返回错误。
    pub fn update(&mut self, data: PurchasePaymentRequestData) -> Result<()> {
        self.ensure_editable()?;
        let data = data.normalize()?;
        let total = sum(data.lines.iter().map(|line| line.amount))?;
        self.supplier_id = data.supplier_id.clone();
        self.data = data;
        self.total_amount = total;
        self.status = PurchasePaymentRequestStatus::Draft;
        Ok(())
    }

    /// 将已证明的来源和主体展示到草稿，不占用来源余额或启动审批。
    ///
    /// # 参数
    /// * `submission` - 当前表单逐采购核验后的完整来源依据。
    /// # 返回
    /// 成功时替换当前草稿依据，提交版本、审批实例和业务状态保持原值。
    /// # 错误
    /// 非可编辑状态、金额、来源、主体或账户不一致时返回错误。
    pub fn set_draft_basis(&mut self, submission: PaymentRequestSubmission) -> Result<()> {
        self.ensure_editable()?;
        validate_submission(self, &submission)?;
        self.total_amount = sum(submission.lines.iter().map(|line| line.requested_amount))?;
        self.assign_basis(submission);
        Ok(())
    }

    /// 冻结本次来源与账户并开启新的审批版本。
    ///
    /// # 参数
    /// * `submission` - 同一事务证明的来源、余额、主体和账户快照。
    /// # 返回
    /// 成功时状态变为审批中，批准与执行绑定清空。
    /// # 错误
    /// 状态、金额、主体、来源、账户不一致或版本溢出时返回错误。
    pub fn submit(&mut self, submission: PaymentRequestSubmission) -> Result<()> {
        self.ensure_editable()?;
        validate_submission(self, &submission)?;
        let version =
            self.submission_version.checked_add(1).ok_or_else(|| Error::from("付款申请提交版本已达上限"))?;
        self.assign_basis(submission);
        self.submission_version = version;
        self.approval_subject_version = version;
        self.approval_instance_id = None;
        self.work_item_id = None;
        self.last_reason = None;
        self.status = PurchasePaymentRequestStatus::InApproval;
        Ok(())
    }

    /// 最终审批通过，只授予已冻结的逐采购额度。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 成功时状态变为待付款，金额不变。
    /// # 错误
    /// 非审批中状态返回错误。
    pub fn approve(&mut self) -> Result<()> {
        self.ensure_in_approval()?;
        self.status = PurchasePaymentRequestStatus::Approved;
        Ok(())
    }

    /// 驳回申请，释放全部未付占用，不自动重启审批。
    ///
    /// # 参数
    /// * `reason` - 必填驳回理由。
    /// # 返回
    /// 成功时变为已驳回，并取消执行绑定。
    /// # 错误
    /// 非审批中或理由无效返回错误。
    pub fn reject(&mut self, reason: &str) -> Result<()> {
        self.stop_approval(PurchasePaymentRequestStatus::Rejected, reason)
    }

    /// 合法撤回审批，释放全部未付占用。
    ///
    /// # 参数
    /// * `reason` - 必填撤回理由。
    /// # 返回
    /// 成功时变为已撤回；重提仍需冻结新版本。
    /// # 错误
    /// 非审批中或理由无效返回错误。
    pub fn withdraw(&mut self, reason: &str) -> Result<()> {
        self.stop_approval(PurchasePaymentRequestStatus::Withdrawn, reason)
    }

    /// 显式终止原批准付款授权，释放未付额度并保留已付款历史。
    ///
    /// # 参数
    /// * `reason` - 必填关闭理由。
    /// # 返回
    /// 返回需在同一事务释放的逐应付金额；已付清返回空集合。
    /// # 错误
    /// 非待付款、部分付款或已付清，或理由无效时返回错误。
    pub fn close(&mut self, reason: &str) -> Result<Vec<PaymentRequestAllocation>> {
        if !self.status.can_close() {
            return Err(Error::from("只有已批准、部分付款或已付清申请可以关闭原付款授权"));
        }
        let reason = required(reason.to_owned(), "关闭原因", 2000)?;
        let released = self.remaining_allocations();
        for line in &mut self.lines {
            line.closed_amount = line.closed_amount.checked_add(line.remaining());
        }
        self.status = PurchasePaymentRequestStatus::Closed;
        self.last_reason = Some(reason);
        Ok(released)
    }

    /// 登记批准范围内实际付款，不允许跨采购转移额度。
    ///
    /// # 参数
    /// * `allocations` - 本次逐应付付款金额。
    /// # 返回
    /// 成功时增加净已付；付清变为已付清，否则为部分付款。
    /// # 错误
    /// 未批准、重复应付、非正金额或超过逐行未付额度时返回错误。
    pub fn record_payment(&mut self, allocations: &[PaymentRequestAllocation]) -> Result<()> {
        self.ensure_payable()?;
        self.validate_allocations(allocations, false)?;
        for allocation in allocations {
            let line = self
                .lines
                .iter_mut()
                .find(|line| line.payable_account_id == allocation.payable_account_id)
                .ok_or_else(|| Error::from("付款采购应付不在申请范围内"))?;
            line.paid_amount = line.paid_amount.checked_add(allocation.amount);
        }
        self.status = if self.remaining() == Amount::zero() {
            PurchasePaymentRequestStatus::Paid
        } else {
            PurchasePaymentRequestStatus::PartiallyPaid
        };
        Ok(())
    }

    /// 记录付款冲正；已关闭申请不恢复原批准授权。
    ///
    /// # 参数
    /// * `allocations` - 原申请各采购行的本次冲正金额。
    /// # 返回
    /// 未关闭申请恢复逐行未付，已关闭申请增加关闭金额。
    /// # 错误
    /// 状态不含实际付款、重复应付或超过净已付金额时返回错误。
    pub fn reverse_payment(&mut self, allocations: &[PaymentRequestAllocation]) -> Result<()> {
        if !matches!(
            self.status,
            PurchasePaymentRequestStatus::Paid
                | PurchasePaymentRequestStatus::PartiallyPaid
                | PurchasePaymentRequestStatus::Closed
        ) {
            return Err(Error::from("付款申请没有可冲正的付款"));
        }
        self.validate_allocations(allocations, true)?;
        for allocation in allocations {
            let line = self
                .lines
                .iter_mut()
                .find(|line| line.payable_account_id == allocation.payable_account_id)
                .ok_or_else(|| Error::from("冲正采购应付不在申请范围内"))?;
            line.paid_amount = line.paid_amount.checked_sub(allocation.amount);
            if self.status == PurchasePaymentRequestStatus::Closed {
                line.closed_amount = line.closed_amount.checked_add(allocation.amount);
            }
        }
        if self.status != PurchasePaymentRequestStatus::Closed {
            self.status = if self.paid_amount() == Amount::zero() {
                PurchasePaymentRequestStatus::Approved
            } else {
                PurchasePaymentRequestStatus::PartiallyPaid
            };
        }
        Ok(())
    }

    /// 返回申请净已付金额。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回各采购行净已付合计。
    /// # 错误
    /// 不返回错误，逐行受申请总额约束。
    pub fn paid_amount(&self) -> Amount {
        self.lines.iter().fold(Amount::zero(), |total, line| total.checked_add(line.paid_amount))
    }

    /// 返回申请已关闭金额。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回各采购行关闭金额合计。
    /// # 错误
    /// 不返回错误。
    pub fn closed_amount(&self) -> Amount {
        self.lines.iter().fold(Amount::zero(), |total, line| total.checked_add(line.closed_amount))
    }

    /// 返回申请尚未支付且未关闭的金额。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回各采购行未付合计。
    /// # 错误
    /// 不返回错误。
    pub fn remaining(&self) -> Amount {
        self.lines.iter().fold(Amount::zero(), |total, line| total.checked_add(line.remaining()))
    }

    /// 返回该申请占用指定应付的未付额度。
    ///
    /// # 参数
    /// * `account_id` - 应付身份。
    /// # 返回
    /// 审批中、待付款和部分付款返回行未付金额；其余状态为零。
    /// # 错误
    /// 不返回错误。
    pub fn reserved_for(&self, account_id: &str) -> Amount {
        if !self.status.occupies() {
            return Amount::zero();
        }
        self.lines
            .iter()
            .find(|line| line.payable_account_id == account_id)
            .map(|line| line.remaining())
            .unwrap_or(Amount::zero())
    }

    /// 返回全部需处理的逐应付未付金额。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回非零未付采购行，保持提交顺序。
    /// # 错误
    /// 不返回错误。
    pub fn remaining_allocations(&self) -> Vec<PaymentRequestAllocation> {
        self.lines
            .iter()
            .filter(|line| line.remaining() > Amount::zero())
            .map(|line| PaymentRequestAllocation {
                payable_account_id: line.payable_account_id.clone(),
                amount: line.remaining(),
            })
            .collect()
    }

    fn ensure_editable(&self) -> Result<()> {
        if !matches!(
            self.status,
            PurchasePaymentRequestStatus::Draft
                | PurchasePaymentRequestStatus::Rejected
                | PurchasePaymentRequestStatus::Withdrawn
        ) {
            return Err(Error::from("只有草稿、驳回或撤回的付款申请可以修改或提交"));
        }
        Ok(())
    }

    fn assign_basis(&mut self, submission: PaymentRequestSubmission) {
        self.supplier_id = submission.supplier_id;
        self.payer_party_id = submission.payer_party_id;
        self.payer_party_name = submission.payer_party_name;
        self.org_unit_id = submission.org_unit_id;
        self.payee_account = Some(submission.payee_account);
        self.lines = submission.lines;
    }

    fn ensure_in_approval(&self) -> Result<()> {
        if self.status != PurchasePaymentRequestStatus::InApproval {
            return Err(Error::from("付款申请不在审批中"));
        }
        Ok(())
    }

    fn ensure_payable(&self) -> Result<()> {
        if !matches!(
            self.status,
            PurchasePaymentRequestStatus::Approved | PurchasePaymentRequestStatus::PartiallyPaid
        ) {
            return Err(Error::from("付款申请尚未批准、已付清或已关闭"));
        }
        Ok(())
    }

    fn stop_approval(&mut self, status: PurchasePaymentRequestStatus, reason: &str) -> Result<()> {
        self.ensure_in_approval()?;
        let reason = required(reason.to_owned(), "处理原因", 2000)?;
        self.status = status;
        self.last_reason = Some(reason);
        self.work_item_id = None;
        Ok(())
    }

    fn validate_allocations(&self, allocations: &[PaymentRequestAllocation], reversing: bool) -> Result<()> {
        if allocations.is_empty() {
            return Err(Error::from("付款或冲正分配不能为空"));
        }
        let mut seen = HashSet::new();
        for allocation in allocations {
            positive(allocation.amount)?;
            if !seen.insert(&allocation.payable_account_id) {
                return Err(Error::from("同一采购应付不得重复分配"));
            }
            let line = self
                .lines
                .iter()
                .find(|line| line.payable_account_id == allocation.payable_account_id)
                .ok_or_else(|| Error::from("采购应付不在付款申请范围内"))?;
            let limit = if reversing { line.paid_amount } else { line.remaining() };
            if allocation.amount > limit {
                return Err(Error::from("金额超过该采购申请行的可执行额度"));
            }
        }
        Ok(())
    }
}

fn validate_submission(
    request: &PurchasePaymentRequest,
    submission: &PaymentRequestSubmission,
) -> Result<()> {
    if submission.supplier_id != request.data.supplier_id
        || submission.payee_account.account_id != request.data.payee_bank_account_id
    {
        return Err(Error::from("付款申请供应商或收款账户与表单不一致"));
    }
    required(submission.payer_party_id.clone(), "我方付款主体", 128)?;
    required(submission.payer_party_name.clone(), "我方付款主体名称", 256)?;
    required(submission.payee_account.supplier_party_id.clone(), "供应商主体", 128)?;
    required(submission.payee_account.account_name.clone(), "收款账户户名", 256)?;
    required(submission.payee_account.bank_name.clone(), "开户行", 256)?;
    required(submission.payee_account.masked_account_number.clone(), "脱敏收款账号", 128)?;
    if submission.payee_account.version == 0 {
        return Err(Error::from("收款账户审核版本无效"));
    }
    if submission.lines.len() != request.data.lines.len() {
        return Err(Error::from("冻结采购范围与表单不一致"));
    }
    let mut accounts = HashSet::new();
    for line in &submission.lines {
        validate_submission_line(request, submission, line)?;
        if !accounts.insert(&line.payable_account_id) {
            return Err(Error::from("同一申请不得重复冻结采购应付"));
        }
    }
    if sum(submission.lines.iter().map(|line| line.requested_amount))? != request.total_amount {
        return Err(Error::from("冻结申请总额与逐采购表单不一致"));
    }
    Ok(())
}

fn validate_submission_line(
    request: &PurchasePaymentRequest,
    submission: &PaymentRequestSubmission,
    line: &super::PurchasePaymentRequestLine,
) -> Result<()> {
    let data = request
        .data
        .lines
        .iter()
        .find(|data| data.payable_account_id == line.payable_account_id)
        .ok_or_else(|| Error::from("冻结采购应付不在申请表单内"))?;
    if data.amount != line.requested_amount
        || line.paid_amount != Amount::zero()
        || line.closed_amount != Amount::zero()
    {
        return Err(Error::from("冻结金额与采购填写的申请金额不一致"));
    }
    required(line.purchase_order_id.clone(), "采购来源", 128)?;
    required(line.purchase_revision_id.clone(), "采购生效版本", 128)?;
    if line.gross_total < Amount::zero()
        || line.settled_total < Amount::zero()
        || line.settled_total > line.gross_total
        || line.gross_total.checked_sub(line.settled_total) != line.open_total
    {
        return Err(Error::from("提交时采购应付总额、有效已付与剩余应付不一致"));
    }
    if line.open_total < Amount::zero()
        || line.other_reserved_total < Amount::zero()
        || line.requested_amount > line.open_total.checked_sub(line.other_reserved_total)
    {
        return Err(Error::from("可申请付款余额不足或应付余额异常"));
    }
    if line.source_snapshot.sales_sources.is_empty() {
        return Err(Error::from("采购付款申请缺少正式销售来源"));
    }
    for source in &line.source_snapshot.sales_sources {
        required(source.sales_order_id.clone(), "销售来源", 128)?;
        required(source.sales_revision_id.clone(), "销售正式版本", 128)?;
        if source.payer_party_id != submission.payer_party_id {
            return Err(Error::from("采购来源跨我方主体，请分组分别申请"));
        }
    }
    Ok(())
}
