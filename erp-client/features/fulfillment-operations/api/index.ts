/**
 * W09 履约单据处理 · 真实 HTTP API。
 *
 * 当前服务端提供采购入库、发货、电子交付和服务履约单据接口，尚未注册
 * W09 专属正式任务。这里仅投影 DRAFT 单据并直达各领域强类型命令；不得把
 * 单据 ID 冒充 work_item，也不得用客户端责任状态补足缺失的任务合同。
 *
 * 实现已按职责拆分：
 * - ./documents  服务端 DTO 形状与「单据 → 工作单」投影
 * - ./hydrate    当前单据的明细补全
 * - ./outcomes   确认后的正式结果投影
 * - ./queue      队列查询与筛选
 * - ./commands   保存 / 确认 / 复核命令
 */

export type { FulfillmentQueueFilters } from "./queue"
export { fetchFulfillmentQueue } from "./queue"
export {
    postFulfillmentOperation,
    resolveUnknownFulfillmentResult,
    saveFulfillmentOperation,
} from "./commands"
