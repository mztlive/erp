//! 商品、可供和履约预检合同；API 来源不能隐式创建公司商品或覆盖销售价。

use std::num::NonZeroU32;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use serde::{Deserialize, Serialize};

use super::address::ConfirmedSupplierAddress;
use super::common::{ActionKey, ConnectorResult, Lookup, Page, Scan, Snapshot, SupplierSku};
use super::order::{DeliveryChoice, OrderLine, Recipient};
use crate::catalog_membership::{CatalogMembershipPage, CatalogPoolScope};
use crate::entity::source_purchase::SourcePurchaseTerms;

/// 目录增量能力必须包含明确的变化覆盖范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalSupport {
    Unsupported,
    /// 包括下架和删除的标识；支持重叠窗口及断点恢复。
    Complete,
    /// 未覆盖全部变化，必须独立核验在售供给，禁止据列表缺失推断删除。
    Partial,
}

/// 来源目录页与可选完整范围证据；缺省 None 不具备按缺项撤回资格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedOfferPage {
    pub page: Page<Snapshot<OfferChange>>,
    /// 已隔离单项的来源身份和脱敏原因；与有效目录及检查点同事务保存。
    pub skipped: Vec<String>,
    pub membership: Option<CatalogMembershipPage>,
}

/// 供应商原始目录读取能力，不是公司商品主档接口。
#[async_trait]
pub trait OfferSource: Send + Sync {
    /// 声明增量查询完整性。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 供应商合同可保证的变化覆盖范围。
    /// # 错误
    /// 不返回错误。
    fn incremental_support(&self) -> IncrementalSupport;

    /// 查询一页商品身份、描述及供给报价。
    ///
    /// # 参数
    /// `scan` 为固定窗口及游标；适配器将供应商时间精度向外扩展，不得漏掉边界。
    /// # 返回
    /// 标准来源目录变化；公司商品、SKU 与供给由公共选品合同严格匹配或新建。
    /// # 错误
    /// 不支持增量时返回 CapabilityGap，不能静默改为全量；其他失败返回分类错误。
    async fn scan(&self, scan: &Scan) -> ConnectorResult<Page<Snapshot<OfferChange>>>;

    /// 读取带完整账号范围及逐池成员证明的目录页。
    /// # 参数
    /// `scan` 为原冻结扫描窗口、页大小与已持久化游标。
    /// # 返回
    /// 目录页及来源范围证据；旧协议默认只返回原目录页并保留范围未知。
    /// # 错误
    /// 来源读取、范围校验或映射失败时不提供可推进的检查点。
    async fn scan_scoped(&self, scan: &Scan) -> ConnectorResult<ScopedOfferPage> {
        Ok(ScopedOfferPage { page: self.scan(scan).await?, skipped: vec![], membership: None })
    }

    /// 重新核验本连接当前完整的来源账号商品池集合。
    /// # 参数
    /// 无；连接及技术账号在适配器实例中绑定。
    /// # 返回
    /// 明确的完整范围，旧协议缺少此证明时返回 None。
    /// # 错误
    /// 鉴权、授权或读取失败时拒绝，不用缓存代替完成前重验。
    async fn scope(&self) -> ConnectorResult<Option<CatalogPoolScope>> {
        Ok(None)
    }

    /// 定向读取一个规格的当前资料。
    ///
    /// # 参数
    /// `sku` 为本连接内的供应商商品和规格身份。
    /// # 返回
    /// 完整报价快照或当前不可见；不可见不等于已删除。
    /// # 错误
    /// 鉴权、映射、限流或传输失败返回分类错误。
    async fn offer(&self, sku: &SupplierSku) -> ConnectorResult<Lookup<Snapshot<Offer>>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfferChange {
    Upsert(Box<Offer>),
    /// 仅在来源明确表示撤回时返回；不能从分页缺失推导。
    Withdrawn(SupplierSku),
}

/// 来源资料供公共选品与差异核对使用；协议适配器不隐式修改公司 SKU 内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub sku: SupplierSku,
    pub name: String,
    pub specification: String,
    /// 来源或显式规格映射未确认时为 None，不猜测为「件」。
    pub unit: Option<String>,
    /// 当前完整供应报价；None 表示未取得，不得用零价替代。
    pub quote: Option<SupplyQuote>,
    /// 来源展示资料及非预占可供观察；缺失字段不表示明确清空。
    pub details: OfferDetails,
}

/// 供应商图片；URL 或图片编号本身不构成内容版本证明。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogImage {
    pub url: String,
    pub content_fingerprint: Option<String>,
    #[serde(default)]
    pub file_asset_id: Option<String>,
    /// 来源主图标记；未取得时保留未知。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_primary: Option<bool>,
    /// 来源明确禁用的图片保留证据，但不归档或采用为公司商品图。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_valid: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sequence: Option<u64>,
    /// 来源相对路径；不得直接作为可访问 URL 使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// 来源商品可供状态，明确撤回与暂不可供分别保存。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CatalogAvailability {
    Available,
    Unavailable,
    Withdrawn,
    #[default]
    Unknown,
}

/// 商品来源资料；图片 None 为未提供，Some([]) 才是明确清空。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferDetails {
    /// 来源购买约束及按真实数量取得的报价范围；缺失保持未知。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_purchase_terms: Option<SourcePurchaseTerms>,
    /// 来源含税市场参考价，与采购报价独立；缺失不推断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_price: Option<Amount>,
    /// 供应商明确提供的实际商品条码；字符串保留前导零，不推导外部订货编号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub brand_id: Option<String>,
    pub brand_name: Option<String>,
    pub description: Option<String>,
    pub images: Option<Vec<CatalogImage>>,
    pub detail_images: Option<Vec<CatalogImage>>,
    pub availability: CatalogAvailability,
    /// 仅精确数量写入；未知不填，不设上限由独立标记表达。
    pub available_quantity: Option<String>,
    pub quantity_unbounded: bool,
}

/// 人民币含税采购报价；不承载供应商建议零售价或公司销售参考价。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplyQuote {
    /// 两种供货价分别取得，不以一种价格自动填充另一种。
    pub dropship: Option<PriceTier>,
    pub bulk: Option<PriceTier>,
    pub tax_rate: Option<Rate>,
    /// 商业有效期，None 表示来源未声明，不表示永久有效。
    pub valid_until: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceTier {
    pub unit_price: UnitPrice,
    /// 未取得来源起购量时为空，不用默认一件补造条件。
    pub minimum_quantity: Option<Quantity>,
}

/// 精确数量、未限制数量、未提供数量分别表达；特殊负数必须按供应商合同解析。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvailableQuantity {
    /// 必须非负；零意味着不能新增购买，仍不代表库存预占保证。
    Exact(Quantity),
    /// 来源明确声明不设数量上限，仍须校验其他可售条件。
    Unbounded,
    Unreported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvailabilityState {
    Available,
    Unavailable { reasons: Vec<AvailabilityBlock> },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityBlock {
    Brand,
    Product,
    Specification,
    Quantity,
    Region,
    DeliverySlot,
}

/// 地区必须来自明确的地区映射；None 仅允许查询与地区无关的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityQuery {
    pub skus: Vec<SupplierSku>,
    pub region: Option<String>,
}

/// 每个请求规格恰有一条结果；缺项必须显式返回 Unknown，不能沿用旧库存。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Availability {
    pub sku: SupplierSku,
    pub state: AvailabilityState,
    pub quantity: AvailableQuantity,
    pub region: Option<String>,
}

/// 标准地区编码集合；必须先完成显式地区映射，不能直接当成本地城市 ID。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionCoverage {
    /// 仅在来源明确不限制地区时使用，仍须核验具体地址和履约时段。
    Unrestricted,
    /// 完整允许集合；空集合表示任何地区均不可售。
    Included(Vec<String>),
    /// 完整排除集合，不得把局部更新当作全量替换。
    Excluded(Vec<String>),
    Unreported,
}

#[async_trait]
pub trait ServiceAreas: Send + Sync {
    /// 获取一个规格当前完整的可售地区，供地区推送补查及发布使用。
    ///
    /// # 参数
    /// `sku` 为已绑定供给的外部商品和规格身份。
    /// # 返回
    /// 合并品牌、商品及规格限制后的完整地区快照；来源缺失返回 Unreported。
    /// # 错误
    /// 地区无法映射、限流或外部查询失败返回分类错误。
    async fn regions(&self, sku: &SupplierSku) -> ConnectorResult<Snapshot<RegionCoverage>>;
}

#[async_trait]
pub trait AvailabilitySource: Send + Sync {
    /// 返回一次查询最多接受的规格数。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已核实的批量上限；不支持批量的供应商返回 1。
    /// # 错误
    /// 不返回错误。
    fn batch_limit(&self) -> NonZeroU32;

    /// 读取可供快照，不预占或扣减供应商库存。
    ///
    /// # 参数
    /// `query` 必须非空、规格不重复且数量不超过 batch_limit。
    /// # 返回
    /// 每个规格的独立来源证据及可供事实。
    /// # 错误
    /// 参数无效、限流、鉴权或读取失败返回分类错误。
    async fn availability(&self, query: &AvailabilityQuery) -> ConnectorResult<Vec<Snapshot<Availability>>>;
}

/// 地址及选项令牌属于供应商协议证据，按敏感数据保存，不派生 Debug。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryContext {
    /// 仅用于绑定连接内恢复地址准备；不得夹带密钥，不能当成通用命令 JSON。
    pub reference: String,
    /// 绑定原加密执行快照的已确认地址；不能从任意 reference 解析自由编码。
    #[serde(default)]
    pub supplier_address: Option<ConfirmedSupplierAddress>,
}

/// 明细必须非空且数量为正，所有 line_id 在本次请求内唯一。
#[derive(Clone, PartialEq, Eq)]
pub struct DeliveryQuery {
    pub context: DeliveryContext,
    pub lines: Vec<OrderLine>,
}

/// 一份完整、可选择的拆单方案；各组无重叠并完整覆盖请求明细。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryPlan {
    pub groups: Vec<DeliveryGroup>,
    /// 供应商承诺的选项有效期；未提供时消费方须采用显式配置的复核策略。
    pub valid_until: Option<Instant>,
}

/// 同一组可以提交为一个外部订单；不同组必须分配不同的我方订单号。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryGroup {
    pub line_ids: Vec<String>,
    pub choices: Vec<DeliveryChoice>,
}

#[async_trait]
pub trait DeliverySource: Send + Sync {
    /// 准备供应商地址/用户引用，单独持久化完成结果，不创建订单或扣款。
    ///
    /// # 参数
    /// `key` 为地址准备动作身份；`recipient` 为本次履约所需的最小收件资料。
    /// # 返回
    /// 后续查询、下单使用的地址上下文；无需远端地址时返回本地协议上下文。
    /// # 错误
    /// 身份冲突或准备失败返回分类错误；结果不明时须恢复原动作，不隐式重建。
    async fn prepare(&self, key: &ActionKey, recipient: &Recipient) -> ConnectorResult<DeliveryContext>;

    /// 恢复原地址准备动作，不再次创建远端用户或地址。
    ///
    /// # 参数
    /// `key` 为原准备动作的稳定身份和载荷指纹。
    /// # 返回
    /// 原上下文或 NotVisible；查不到不得推导可安全重建。
    /// # 错误
    /// 无恢复能力返回 CapabilityGap 并转人工；其他读取失败返回分类错误。
    async fn context(&self, key: &ActionKey) -> ConnectorResult<Lookup<DeliveryContext>>;

    /// 使用原冻结收件资料恢复本地地址上下文，不重新执行地址转换。
    /// # 参数
    /// `key` 为原地址动作，`recipient` 为原加密执行快照中的收件资料。
    /// # 返回
    /// 原上下文；旧协议默认沿用原动作查询。
    /// # 错误
    /// 资料变化、绑定失效或无法证明原结果时拒绝。
    async fn context_with_recipient(
        &self,
        key: &ActionKey,
        _recipient: &Recipient,
    ) -> ConnectorResult<Lookup<DeliveryContext>> {
        self.context(key).await
    }

    /// 查询配送、运费、时段及拆单方案，不创建供应商订单。
    ///
    /// # 参数
    /// `query` 为已准备的地址上下文及完整明细。
    /// # 返回
    /// 完整方案快照；拒绝配送必须返回 BusinessRejected，不能返回空方案伪装成功。
    /// # 错误
    /// 超区、商品不可售、无时段或外部故障返回分类错误。
    async fn options(&self, query: &DeliveryQuery) -> ConnectorResult<Snapshot<DeliveryPlan>>;
}
