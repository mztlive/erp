//! 销售正式版本聚合工厂。
//!
//! 首次提交与销售变更提交共用同一套公共行、快照和卡券单行约束；ID、时间、版本号
//! 与上一版本指针由调用方注入，本模块不查询仓储、不生成 ID。

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    ContractRevisionId, SalesOrderGoodsServiceLineRevisionId, SalesOrderId, SalesOrderLineId,
    SalesOrderRevisionId, SalesOrderRevisionLineId, SalesOrderVoucherLineRevisionId, SkuId,
};
use crate::money::{Amount, Rate};

use super::revision::{
    RevisionSource, SalesOrderGoodsServiceLineRevision, SalesOrderGoodsServiceLineRevisionData,
    SalesOrderRevision, SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    SalesOrderVoucherLineRevision, SalesOrderVoucherLineRevisionData,
};
use super::snapshot::HeaderSnapshotData;
use super::submission::{SalesOrderSubmission, SalesOrderSubmissionLine};
use super::types::{
    validate_line_list, BusinessType, GoodsLineFields, LineSummary, LineType, VoucherLineDraft,
};

/// 调用方注入的正式版本稳定身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormalRevisionIdentities {
    /// 正式版本头身份。
    pub revision_id: SalesOrderRevisionId,
    /// 与提交行一一对应的公共行及子类型身份。
    pub lines: Vec<FormalRevisionLineIdentity>,
}

impl FormalRevisionIdentities {
    /// 用调用方已生成的稳定身份构造正式版本身份清单。
    ///
    /// # 参数
    /// * `revision_id` - 正式版本头身份
    /// * `lines` - 与提交行顺序一致的行身份
    ///
    /// # 返回
    /// 返回身份清单；本方法不校验数量或行类型。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 数量与行类型必须在聚合工厂内与提交行对齐，调用方不得事后改写。
    pub fn new(revision_id: SalesOrderRevisionId, lines: Vec<FormalRevisionLineIdentity>) -> Self {
        Self { revision_id, lines }
    }

    /// 校验身份数量和子类型与已规范化行一致。
    ///
    /// # 参数
    /// * `lines` - 已规范化的正式版本行输入
    ///
    /// # 返回
    /// 数量和行类型均对齐时返回 `Ok(())`。
    ///
    /// # 错误
    /// 数量不一致或某行身份与行类型不符时返回领域错误。
    ///
    /// # 关键业务约束
    /// 身份必须由调用方预先分配；本方法不得补齐或改写 ID。
    fn ensure_matches_lines(&self, lines: &[PreparedRevisionLine]) -> Result<()> {
        if self.lines.len() != lines.len() {
            return Err(Error::from("正式版本身份数量必须与明细行数量一致"));
        }
        for (identity, line) in self.lines.iter().zip(lines) {
            if !identity.subtype.matches_line_type(line.line_type) {
                return Err(Error::from(format!(
                    "第 {} 行的正式版本身份与行类型不一致",
                    line.line_no
                )));
            }
        }
        Ok(())
    }
}

/// 单行正式版本的公共行与子类型身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormalRevisionLineIdentity {
    /// 公共行版本身份。
    pub revision_line_id: SalesOrderRevisionLineId,
    /// 与行类型对应的子类型身份。
    pub subtype: FormalRevisionSubtypeIdentity,
}

impl FormalRevisionLineIdentity {
    /// 用调用方已生成的公共行和子类型身份构造一行正式版本身份。
    ///
    /// # 参数
    /// * `revision_line_id` - 公共行版本身份
    /// * `subtype` - 子类型身份
    ///
    /// # 返回
    /// 返回单行身份。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 子类型必须与对应提交行的 `line_type` 一致。
    pub fn new(revision_line_id: SalesOrderRevisionLineId, subtype: FormalRevisionSubtypeIdentity) -> Self {
        Self {
            revision_line_id,
            subtype,
        }
    }
}

/// 正式版本子类型行身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormalRevisionSubtypeIdentity {
    /// 实物及服务子行身份。
    GoodsService(SalesOrderGoodsServiceLineRevisionId),
    /// 卡券子行身份。
    Voucher(SalesOrderVoucherLineRevisionId),
}

impl FormalRevisionSubtypeIdentity {
    /// 按行类型用调用方已生成的 ID 构造子类型身份。
    ///
    /// # 参数
    /// * `line_type` - 提交行类型
    /// * `id` - 子类型行稳定身份
    ///
    /// # 返回
    /// 返回与行类型对应的子类型身份。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 实物行不得持有卡券身份，卡券行不得持有实物身份。
    pub fn from_line_type(line_type: LineType, id: impl Into<String>) -> Self {
        match line_type {
            LineType::GoodsService => {
                Self::GoodsService(SalesOrderGoodsServiceLineRevisionId::new(id.into()))
            }
            LineType::Voucher => Self::Voucher(SalesOrderVoucherLineRevisionId::new(id.into())),
        }
    }

    /// 判断子类型身份是否匹配给定行类型。
    ///
    /// # 参数
    /// * `line_type` - 提交行类型
    ///
    /// # 返回
    /// 身份与行类型一致时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 无。
    fn matches_line_type(&self, line_type: LineType) -> bool {
        matches!(
            (self, line_type),
            (Self::GoodsService(_), LineType::GoodsService) | (Self::Voucher(_), LineType::Voucher)
        )
    }
}

/// 正式版本构造所需的调用方上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormalRevisionContext {
    /// 聚合内版本号；首次生效由调用方传入 `1`，变更生效传入已查询的下一号。
    pub revision_no: u32,
    /// 版本来源。
    pub revision_source: RevisionSource,
    /// 前一生效版本；尚无正式版本时为空。
    pub previous_revision_id: Option<SalesOrderRevisionId>,
    /// 销售单业务性质，用于行类型约束。
    pub business_type: BusinessType,
    /// 生效与入账时间。
    pub effective_at: Instant,
}

impl FormalRevisionContext {
    /// 构造正式版本上下文。
    ///
    /// # 参数
    /// * `revision_no` - 调用方确定的下一版本号
    /// * `revision_source` - 版本来源
    /// * `previous_revision_id` - 当前生效版本指针
    /// * `business_type` - 销售单业务性质
    /// * `effective_at` - 生效时间，同时作为入账时间
    ///
    /// # 返回
    /// 返回上下文；本方法不校验版本号。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 版本号与上一版本指针由调用方从仓储事实得到，工厂不得查询 latest revision no。
    pub fn new(
        revision_no: u32,
        revision_source: RevisionSource,
        previous_revision_id: Option<SalesOrderRevisionId>,
        business_type: BusinessType,
        effective_at: Instant,
    ) -> Self {
        Self {
            revision_no,
            revision_source,
            previous_revision_id,
            business_type,
            effective_at,
        }
    }
}

/// 销售正式版本聚合（版本头 + 公共行 + 子类型行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesOrderRevisionAggregate {
    /// 版本头。
    pub revision: SalesOrderRevision,
    /// 公共行版本，顺序与输入行一致。
    pub lines: Vec<SalesOrderRevisionLine>,
    /// 实物及服务子行。
    pub goods_lines: Vec<SalesOrderGoodsServiceLineRevision>,
    /// 卡券子行。
    pub voucher_lines: Vec<SalesOrderVoucherLineRevision>,
}

/// 已清洗的表头输入，供首次提交与变更提交共用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormalRevisionHeader {
    /// 稳定销售单。
    pub sales_order_id: SalesOrderId,
    /// 内容指纹，当前合同为 `sub:{submission_id}`。
    pub content_hash: String,
    /// 生效时合同版本。
    pub contract_revision_id: Option<ContractRevisionId>,
    /// 表头结构化快照。
    pub snapshot: HeaderSnapshotData,
    /// 客户项目名称。
    pub project_name: Option<String>,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU。
    pub voucher_category_sku_id: Option<SkuId>,
    /// 卡券履约期限。
    pub voucher_expiry_at: Option<Instant>,
    /// 已舍入行汇总（含税）。
    pub gross_amount: Amount,
    /// 已舍入行汇总（不含税）。
    pub net_amount: Amount,
    /// 已舍入行汇总（税额）。
    pub tax_amount: Amount,
}

impl FormalRevisionHeader {
    /// 从首次销售提交复制表头快照、金额和内容指纹。
    ///
    /// # 参数
    /// * `submission` - 已冻结销售提交
    ///
    /// # 返回
    /// 返回正式版本表头输入。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 指纹保持历史 `sub:{id}` 形态，不在本批更换算法。
    pub(crate) fn from_sales_order_submission(submission: &SalesOrderSubmission) -> Self {
        Self {
            sales_order_id: submission.sales_order_id.clone(),
            content_hash: submission_content_hash(&submission.base.id),
            contract_revision_id: submission.contract_revision_id.clone(),
            snapshot: submission.header_snapshot_data(),
            project_name: submission.project_name.clone(),
            business_remark: submission.business_remark.clone(),
            voucher_category_sku_id: submission.voucher_category_sku_id.clone(),
            voucher_expiry_at: submission.voucher_expiry_at,
            gross_amount: submission.gross_amount,
            net_amount: submission.net_amount,
            tax_amount: submission.tax_amount,
        }
    }
}

/// 已规范化的正式版本行输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedRevisionLine {
    /// 稳定明细身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: LineType,
    /// 行含税金额。
    pub gross_amount: Amount,
    /// 行不含税金额。
    pub net_amount: Amount,
    /// 行税额。
    pub tax_amount: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 销售项名称快照。
    pub item_name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 单位快照。
    pub unit_snapshot: Option<String>,
    /// 实物及服务字段组。
    pub goods: Option<GoodsLineFields>,
    /// 卡券字段组。
    pub voucher: Option<VoucherLineDraft>,
}

impl PreparedRevisionLine {
    /// 从首次销售提交行还原正式版本行输入。
    ///
    /// # 参数
    /// * `line` - 已冻结提交行
    ///
    /// # 返回
    /// 返回带完整字段组的行输入。
    ///
    /// # 错误
    /// 行类型与字段组不一致或必填字段缺失时返回领域错误。
    ///
    /// # 关键业务约束
    /// 字段组必须通过提交行实体方法还原，禁止在工厂内重新拆 Optional。
    fn from_sales_order_submission_line(line: &SalesOrderSubmissionLine) -> Result<Self> {
        let (goods, voucher) = match line.line_type {
            LineType::GoodsService => (Some(line.goods_fields()?), None),
            LineType::Voucher => (None, Some(line.voucher_fields()?)),
        };
        Ok(Self {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type: line.line_type,
            gross_amount: line.gross_amount,
            net_amount: line.net_amount,
            tax_amount: line.tax_amount,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods,
            voucher,
        })
    }
}

impl SalesOrderRevisionAggregate {
    /// 由首次销售提交构造正式版本聚合。
    ///
    /// # 参数
    /// * `identities` - 调用方注入的版本与行身份
    /// * `context` - 版本号、来源、上一版本与业务性质
    /// * `submission` - 已冻结销售提交
    /// * `lines` - 该提交的全部明细行
    ///
    /// # 返回
    /// 返回一次构造完成的版本头、公共行和子类型行。
    ///
    /// # 错误
    /// 业务性质漂移、空行、混合行、多卡券行、字段组缺失、身份数量/类型不符或
    /// 金额/快照不合法时返回领域错误。
    ///
    /// # 关键业务约束
    /// 卡券单必须且只能包含一条卡券明细；工厂不查询仓储、不生成 ID。
    pub fn from_sales_order_submission(
        identities: FormalRevisionIdentities,
        context: FormalRevisionContext,
        submission: &SalesOrderSubmission,
        lines: &[SalesOrderSubmissionLine],
    ) -> Result<Self> {
        ensure_business_type(context.business_type, submission.business_type)?;
        let prepared = lines
            .iter()
            .map(PreparedRevisionLine::from_sales_order_submission_line)
            .collect::<Result<Vec<_>>>()?;
        Self::from_prepared(
            identities,
            context,
            FormalRevisionHeader::from_sales_order_submission(submission),
            prepared,
        )
    }

    /// 由已规范化的表头和行构造正式版本聚合。
    ///
    /// # 参数
    /// * `identities` - 调用方注入的版本与行身份
    /// * `context` - 版本号、来源、上一版本与业务性质
    /// * `header` - 已复制的表头快照与金额
    /// * `lines` - 已还原字段组的行输入
    ///
    /// # 返回
    /// 返回完整正式版本聚合。
    ///
    /// # 错误
    /// 行清单、身份对齐、快照或金额不合法时返回领域错误。
    ///
    /// # 关键业务约束
    /// 首次提交与变更提交必须走本入口，避免双规则源。
    pub(crate) fn from_prepared(
        identities: FormalRevisionIdentities,
        context: FormalRevisionContext,
        header: FormalRevisionHeader,
        lines: Vec<PreparedRevisionLine>,
    ) -> Result<Self> {
        identities.ensure_matches_lines(&lines)?;
        validate_line_list(context.business_type, &line_summaries(&lines))?;
        let revision = build_revision_header(&identities.revision_id, &context, header)?;
        let (revision_lines, goods_lines, voucher_lines) =
            build_revision_children(&identities.revision_id, &identities.lines, lines)?;
        Ok(Self {
            revision,
            lines: revision_lines,
            goods_lines,
            voucher_lines,
        })
    }
}

/// 校验销售单业务性质与提交业务性质一致。
///
/// # 参数
/// * `expected` - 销售单业务性质
/// * `actual` - 提交业务性质
///
/// # 返回
/// 两者一致时返回 `Ok(())`。
///
/// # 错误
/// 不一致时返回领域错误。
///
/// # 关键业务约束
/// 正式版本行类型约束以销售单业务性质为准，提交不得漂移。
fn ensure_business_type(expected: BusinessType, actual: BusinessType) -> Result<()> {
    if expected != actual {
        return Err(Error::from("销售单业务性质与提交不一致"));
    }
    Ok(())
}

/// 把已规范化行转换为行清单摘要。
///
/// # 参数
/// * `lines` - 已规范化行
///
/// # 返回
/// 返回供 `validate_line_list` 使用的摘要。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 摘要只含行号、稳定身份和行类型，不含金额。
fn line_summaries(lines: &[PreparedRevisionLine]) -> Vec<LineSummary> {
    lines
        .iter()
        .map(|line| LineSummary {
            line_no: line.line_no,
            line_id: line.sales_order_line_id.clone(),
            line_type: line.line_type,
        })
        .collect()
}

/// 由提交主键生成正式版本内容指纹。
///
/// # 参数
/// * `submission_id` - 首次提交或变更提交主键
///
/// # 返回
/// 返回 `sub:{id}` 历史形态指纹。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 本批不得改写已持久化 `sub:` 前缀合同。
pub(crate) fn submission_content_hash(submission_id: &str) -> String {
    format!("sub:{submission_id}")
}

/// 构造正式版本头。
///
/// # 参数
/// * `revision_id` - 版本头身份
/// * `context` - 版本号、来源、上一版本和时间
/// * `header` - 表头快照与金额
///
/// # 返回
/// 返回不可变版本头。
///
/// # 错误
/// 版本号、指纹、快照或金额三元组不合法时返回领域错误。
///
/// # 关键业务约束
/// `customer_revision_id` 与商城快照保持空值，与现行首次/变更生效路径一致。
fn build_revision_header(
    revision_id: &SalesOrderRevisionId,
    context: &FormalRevisionContext,
    header: FormalRevisionHeader,
) -> Result<SalesOrderRevision> {
    SalesOrderRevision::new(
        revision_id.clone(),
        SalesOrderRevisionData {
            sales_order_id: header.sales_order_id,
            revision_no: context.revision_no,
            revision_source: context.revision_source,
            source_snapshot_id: None,
            previous_revision_id: context.previous_revision_id.clone(),
            content_hash: header.content_hash,
            customer_revision_id: None,
            contract_revision_id: header.contract_revision_id,
            snapshot: header.snapshot,
            project_name: header.project_name,
            business_remark: header.business_remark,
            voucher_category_sku_id: header.voucher_category_sku_id,
            voucher_expiry_at: header.voucher_expiry_at,
            gross_amount: header.gross_amount,
            net_amount: header.net_amount,
            tax_amount: header.tax_amount,
            effective_at: context.effective_at,
            recorded_at: context.effective_at,
        },
    )
}

/// 按身份顺序构造公共行与子类型行。
///
/// # 参数
/// * `revision_id` - 版本头身份
/// * `identities` - 行身份清单
/// * `lines` - 已规范化行
///
/// # 返回
/// 返回 `(公共行, 实物子行, 卡券子行)`。
///
/// # 错误
/// 公共行或子类型行构造失败时返回领域错误。
///
/// # 关键业务约束
/// 行顺序保持输入顺序，不得按行号重排。
fn build_revision_children(
    revision_id: &SalesOrderRevisionId,
    identities: &[FormalRevisionLineIdentity],
    lines: Vec<PreparedRevisionLine>,
) -> Result<(
    Vec<SalesOrderRevisionLine>,
    Vec<SalesOrderGoodsServiceLineRevision>,
    Vec<SalesOrderVoucherLineRevision>,
)> {
    let mut revision_lines = Vec::with_capacity(lines.len());
    let mut goods_lines = Vec::new();
    let mut voucher_lines = Vec::new();
    for (identity, line) in identities.iter().zip(lines) {
        revision_lines.push(build_common_line(revision_id, identity, &line)?);
        append_subtype_line(identity, line, &mut goods_lines, &mut voucher_lines)?;
    }
    Ok((revision_lines, goods_lines, voucher_lines))
}

/// 构造正式版本公共行。
///
/// # 参数
/// * `revision_id` - 版本头身份
/// * `identity` - 当前行身份
/// * `line` - 已规范化行
///
/// # 返回
/// 返回公共行版本。
///
/// # 错误
/// 行号、名称快照或金额三元组不合法时返回领域错误。
///
/// # 关键业务约束
/// 公共行金额必须来自提交行已舍入值，工厂不得重算。
fn build_common_line(
    revision_id: &SalesOrderRevisionId,
    identity: &FormalRevisionLineIdentity,
    line: &PreparedRevisionLine,
) -> Result<SalesOrderRevisionLine> {
    SalesOrderRevisionLine::new(
        identity.revision_line_id.clone(),
        SalesOrderRevisionLineData {
            sales_order_revision_id: revision_id.clone(),
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type: line.line_type,
            gross_amount: line.gross_amount,
            net_amount: line.net_amount,
            tax_amount: line.tax_amount,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
        },
    )
}

/// 按行类型追加实物或卡券子行。
///
/// # 参数
/// * `identity` - 当前行身份
/// * `line` - 已规范化行
/// * `goods_lines` - 实物子行累加器
/// * `voucher_lines` - 卡券子行累加器
///
/// # 返回
/// 追加成功时返回 `Ok(())`。
///
/// # 错误
/// 字段组与行类型不一致或子行构造失败时返回领域错误。
///
/// # 关键业务约束
/// 子行必须一对一引用公共行身份。
fn append_subtype_line(
    identity: &FormalRevisionLineIdentity,
    line: PreparedRevisionLine,
    goods_lines: &mut Vec<SalesOrderGoodsServiceLineRevision>,
    voucher_lines: &mut Vec<SalesOrderVoucherLineRevision>,
) -> Result<()> {
    match (&identity.subtype, line.goods, line.voucher) {
        (FormalRevisionSubtypeIdentity::GoodsService(id), Some(goods), None) => {
            goods_lines.push(build_goods_line(id, &identity.revision_line_id, goods)?);
            Ok(())
        }
        (FormalRevisionSubtypeIdentity::Voucher(id), None, Some(voucher)) => {
            voucher_lines.push(build_voucher_line(id, &identity.revision_line_id, voucher)?);
            Ok(())
        }
        _ => Err(Error::from(format!("第 {} 行字段组与行类型不一致", line.line_no))),
    }
}

/// 构造实物及服务子行。
///
/// # 参数
/// * `id` - 子行身份
/// * `revision_line_id` - 对应公共行身份
/// * `goods` - 已还原实物字段组
///
/// # 返回
/// 返回实物子行。
///
/// # 错误
/// 基础单位非法时返回领域错误。
///
/// # 关键业务约束
/// 单价与数量快照保持提交值，不随后续 SKU 价格变化。
fn build_goods_line(
    id: &SalesOrderGoodsServiceLineRevisionId,
    revision_line_id: &SalesOrderRevisionLineId,
    goods: GoodsLineFields,
) -> Result<SalesOrderGoodsServiceLineRevision> {
    SalesOrderGoodsServiceLineRevision::new(
        id.clone(),
        SalesOrderGoodsServiceLineRevisionData {
            revision_line_id: revision_line_id.clone(),
            sku_id: goods.sku_id,
            sku_revision_id: goods.sku_revision_id,
            welfare_scenario: goods.welfare_scenario,
            service_region: goods.service_region,
            fulfillment_due_at: goods.fulfillment_due_at,
            quantity: goods.quantity,
            base_unit_code: goods.base_unit_code,
            unit_price_gross: goods.unit_price_gross,
        },
    )
}

/// 构造卡券子行。
///
/// # 参数
/// * `id` - 子行身份
/// * `revision_line_id` - 对应公共行身份
/// * `voucher` - 已还原卡券字段组
///
/// # 返回
/// 返回卡券子行。
///
/// # 错误
/// 卡张数为零或成交金额为零时返回领域错误。
///
/// # 关键业务约束
/// 面额小计、成交金额和配赠由卡券行实体按公式推导。
fn build_voucher_line(
    id: &SalesOrderVoucherLineRevisionId,
    revision_line_id: &SalesOrderRevisionLineId,
    voucher: VoucherLineDraft,
) -> Result<SalesOrderVoucherLineRevision> {
    SalesOrderVoucherLineRevision::new(
        id.clone(),
        SalesOrderVoucherLineRevisionData {
            revision_line_id: revision_line_id.clone(),
            face_value: voucher.face_value,
            card_count: voucher.card_count,
            unit_price_gross: voucher.unit_price_gross,
            card_form: voucher.card_form,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::ids::{
        CustomerAccountId, PartyId, SalesOrderSubmissionId, SalesOrderSubmissionLineId, SkuRevisionId,
        SourceSystemId,
    };
    use crate::money::{Quantity, UnitPrice};
    use crate::sales_order::submission::SalesOrderSubmissionData;
    use crate::sales_order::types::{CardForm, WelfareScenario};

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn rate(value: &str) -> Rate {
        Rate::from_str(value).unwrap()
    }

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn price(value: &str) -> UnitPrice {
        UnitPrice::from_str(value).unwrap()
    }

    fn at() -> Instant {
        Instant::from_unix_secs(1_800_000_000)
    }

    fn goods_fields() -> GoodsLineFields {
        GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
            service_region: Some("east".to_string()),
            fulfillment_due_at: at(),
            quantity: qty("3.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        }
    }

    fn goods_line_data(line_no: u32) -> super::super::submission::SalesOrderSubmissionLineData {
        super::super::submission::SalesOrderSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: "年货礼盒".to_string(),
            spec_snapshot: Some("10kg".to_string()),
            unit_snapshot: Some("箱".to_string()),
            goods: Some(goods_fields()),
            voucher: None,
        }
    }

    fn voucher_line_data(line_no: u32) -> super::super::submission::SalesOrderSubmissionLineData {
        super::super::submission::SalesOrderSubmissionLineData {
            line_type: LineType::Voucher,
            goods: None,
            voucher: Some(VoucherLineDraft {
                face_value: amt("100.00"),
                card_count: 3,
                unit_price_gross: price("90.0000"),
                face_value_total: amt("300.00"),
                transaction_amount: amt("270.00"),
                gift_amount: amt("30.00"),
                gift_rate: None,
                card_form: CardForm::Electronic,
            }),
            ..goods_line_data(line_no)
        }
    }

    fn header_snapshot() -> HeaderSnapshotData {
        HeaderSnapshotData {
            customer_name: "东方企业".to_string(),
            contract_no: Some("HT-2026-0088".to_string()),
            settlement_party_name: Some("集团结算中心".to_string()),
            payment_term_code: "NET30".to_string(),
            payment_term_name: "月结 30 天".to_string(),
            invoice_type: "增值税专用发票".to_string(),
            tax_point: "6".to_string(),
        }
    }

    fn goods_header(
        lines: Vec<super::super::submission::SalesOrderSubmissionLineData>,
    ) -> SalesOrderSubmissionData {
        let (gross_amount, net_amount, tax_amount) = match lines.len() {
            1 => (amt("29.97"), amt("26.07"), amt("3.90")),
            2 => (amt("59.94"), amt("52.14"), amt("7.80")),
            _ => (amt("29.97"), amt("26.07"), amt("3.90")),
        };
        SalesOrderSubmissionData {
            sales_order_id: SalesOrderId::new("o-1"),
            submission_no: 1,
            working_copy_id: crate::ids::SalesOrderWorkingCopyId::new("wc-1"),
            working_copy_version: 3,
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            settlement_party_id: PartyId::new("party-1"),
            snapshot: header_snapshot(),
            project_name: Some("端午福利项目".to_string()),
            business_remark: Some("按合同执行".to_string()),
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            target_mall_id: None,
            customer_external_identity: None,
            voucher_category_external_identity: None,
            receivable_due_date: None,
            gross_amount,
            net_amount,
            tax_amount,
            submitted_at: at(),
            submitted_by: "sales-1".to_string(),
            lines,
        }
    }

    fn voucher_header(
        lines: Vec<super::super::submission::SalesOrderSubmissionLineData>,
    ) -> SalesOrderSubmissionData {
        SalesOrderSubmissionData {
            business_type: BusinessType::Voucher,
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: Some(Instant::from_unix_secs(1_850_000_000)),
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            customer_external_identity: Some("mall-customer-1".to_string()),
            voucher_category_external_identity: Some("mall-voucher-1".to_string()),
            receivable_due_date: Some(crate::common::time::BusinessDate::from_ymd(2026, 10, 31).unwrap()),
            submitted_at: Instant::from_unix_secs(1_790_000_000),
            gross_amount: amt("270.00"),
            net_amount: amt("238.94"),
            tax_amount: amt("31.06"),
            lines,
            ..goods_header(vec![goods_line_data(1)])
        }
    }

    fn submission_and_lines(
        data: SalesOrderSubmissionData,
    ) -> (SalesOrderSubmission, Vec<SalesOrderSubmissionLine>) {
        let line_data = data.lines.clone();
        let submission = SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), data).unwrap();
        let lines = line_data
            .into_iter()
            .enumerate()
            .map(|(index, data)| {
                SalesOrderSubmissionLine::new(
                    SalesOrderSubmissionLineId::new(format!("sl-{index}")),
                    SalesOrderSubmissionId::new("s-1"),
                    data,
                )
                .unwrap()
            })
            .collect();
        (submission, lines)
    }

    fn identities_for(line_types: &[LineType]) -> FormalRevisionIdentities {
        FormalRevisionIdentities::new(
            SalesOrderRevisionId::new("rev-new"),
            line_types
                .iter()
                .enumerate()
                .map(|(index, line_type)| {
                    FormalRevisionLineIdentity::new(
                        SalesOrderRevisionLineId::new(format!("rl-{index}")),
                        FormalRevisionSubtypeIdentity::from_line_type(*line_type, format!("st-{index}")),
                    )
                })
                .collect(),
        )
    }

    fn goods_context() -> FormalRevisionContext {
        FormalRevisionContext::new(
            1,
            RevisionSource::ErpApproval,
            None,
            BusinessType::GoodsService,
            at(),
        )
    }

    fn voucher_context() -> FormalRevisionContext {
        FormalRevisionContext::new(1, RevisionSource::ErpApproval, None, BusinessType::Voucher, at())
    }

    #[test]
    fn goods_service_multi_line_revision_copies_snapshots_and_hashes() {
        let (submission, lines) =
            submission_and_lines(goods_header(vec![goods_line_data(1), goods_line_data(2)]));
        let aggregate = SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[LineType::GoodsService, LineType::GoodsService]),
            FormalRevisionContext::new(
                1,
                RevisionSource::ErpApproval,
                Some(SalesOrderRevisionId::new("rev-prev")),
                BusinessType::GoodsService,
                at(),
            ),
            &submission,
            &lines,
        )
        .unwrap();

        assert_eq!(aggregate.revision.revision.revision_no, 1);
        assert_eq!(aggregate.revision.revision_source, RevisionSource::ErpApproval);
        assert_eq!(
            aggregate.revision.previous_revision_id,
            Some(SalesOrderRevisionId::new("rev-prev"))
        );
        assert_eq!(aggregate.revision.content_hash, "sub:s-1");
        assert_eq!(aggregate.revision.customer_snapshot.customer_name, "东方企业");
        assert_eq!(aggregate.revision.project_name.as_deref(), Some("端午福利项目"));
        assert_eq!(aggregate.revision.gross_amount, amt("59.94"));
        assert_eq!(aggregate.lines.len(), 2);
        assert_eq!(aggregate.goods_lines.len(), 2);
        assert!(aggregate.voucher_lines.is_empty());
        assert_eq!(aggregate.lines[0].line_no, 1);
        assert_eq!(aggregate.lines[1].line_no, 2);
        assert_eq!(
            aggregate.goods_lines[0].welfare_scenario,
            Some(WelfareScenario::AnnualGiftBag)
        );
        assert_eq!(aggregate.goods_lines[0].service_region.as_deref(), Some("EAST"));
    }

    #[test]
    fn voucher_single_line_revision_keeps_card_fields() {
        let (submission, lines) = submission_and_lines(voucher_header(vec![voucher_line_data(1)]));
        let aggregate = SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[LineType::Voucher]),
            voucher_context(),
            &submission,
            &lines,
        )
        .unwrap();

        assert_eq!(aggregate.lines.len(), 1);
        assert!(aggregate.goods_lines.is_empty());
        assert_eq!(aggregate.voucher_lines.len(), 1);
        assert_eq!(aggregate.voucher_lines[0].card_count, 3);
        assert_eq!(aggregate.voucher_lines[0].card_form, CardForm::Electronic);
        assert_eq!(
            aggregate.revision.voucher_category_sku_id,
            Some(SkuId::new("vcat-1"))
        );
    }

    #[test]
    fn factory_rejects_empty_mixed_and_multi_voucher_lines() {
        let (submission, lines) = submission_and_lines(goods_header(vec![goods_line_data(1)]));
        assert!(SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[]),
            goods_context(),
            &submission,
            &[],
        )
        .is_err());

        let voucher_line = SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("sl-v"),
            SalesOrderSubmissionId::new("s-1"),
            voucher_line_data(2),
        )
        .unwrap();
        assert!(SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[LineType::GoodsService, LineType::Voucher]),
            goods_context(),
            &submission,
            &[lines[0].clone(), voucher_line],
        )
        .is_err());

        let (voucher_submission, _) = submission_and_lines(voucher_header(vec![voucher_line_data(1)]));
        let first = SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("sl-a"),
            SalesOrderSubmissionId::new("s-1"),
            voucher_line_data(1),
        )
        .unwrap();
        let mut second = first.clone();
        second.base.id = "sl-b".to_string();
        second.sales_order_line_id = SalesOrderLineId::new("line-2");
        second.line_no = 2;
        assert!(SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[LineType::Voucher, LineType::Voucher]),
            voucher_context(),
            &voucher_submission,
            &[first, second],
        )
        .is_err());
    }

    #[test]
    fn factory_rejects_identity_mismatch_and_business_type_drift() {
        let (submission, lines) = submission_and_lines(goods_header(vec![goods_line_data(1)]));
        assert!(SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[LineType::GoodsService, LineType::GoodsService]),
            goods_context(),
            &submission,
            &lines,
        )
        .is_err());

        assert!(SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[LineType::Voucher]),
            goods_context(),
            &submission,
            &lines,
        )
        .is_err());

        assert!(SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[LineType::GoodsService]),
            voucher_context(),
            &submission,
            &lines,
        )
        .is_err());
    }

    #[test]
    fn factory_rejects_missing_goods_field_group() {
        let (submission, mut lines) = submission_and_lines(goods_header(vec![goods_line_data(1)]));
        lines[0].sku_id = None;
        assert!(SalesOrderRevisionAggregate::from_sales_order_submission(
            identities_for(&[LineType::GoodsService]),
            goods_context(),
            &submission,
            &lines,
        )
        .is_err());
    }
}
