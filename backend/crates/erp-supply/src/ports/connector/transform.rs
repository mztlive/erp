//! 供应商原始商品到标准来源的转换合同；输入类型由各适配器定义。

use super::common::ConnectorResult;
use super::offer::Offer;

/// 商品适配器的统一转换入口；只转换已取得的数据，不执行网络请求或数据库写入。
///
/// `Source` 可以是供应商 DTO 的引用或包含多个借用的输入结构，不要求统一原始报文格式。
/// 读取接口负责取得原始数据并调用本接口；目录保存和公司商品建档由公共流程执行。
pub trait OfferTransform<Source>: Send + Sync {
    /// 将一个供应商规格转换为标准商品来源。
    ///
    /// # 参数
    /// `source` 为供应商原始商品、规格及本次响应中的相关资料。
    /// # 返回
    /// 标准 `Offer`；来源未提供的可选字段保留缺失，不伪造价格、单位或库存。
    /// # 错误
    /// 身份、必填字段或数值无效时返回映射错误，保留供应商适配器的错误代码和语义。
    fn transform(&self, source: Source) -> ConnectorResult<Offer>;
}
