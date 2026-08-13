import type { DisplayTime } from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import type {
    CardSalesApproval,
    SalesOrderListItem,
} from "@/features/sales-orders/types"

/** 业务性质展示文案（W05 列表/详情/纸质预览共用）。 */
export const NATURE_LABEL: Record<SalesOrderListItem["nature"], string> = {
    physical_service: "实物与服务",
    card_voucher: "卡券",
}

/** 阶段责任角色中文映射（后端固定码，见 `sales_order/mod.rs` 提交编排）。 */
const STAGE_OWNER_ROLE_LABEL: Record<string, string> = {
    procurement: "采购",
    sales_leader: "销售领导",
    operations: "运营",
}

/** 审核轨进行中的阶段码（草稿/已生效/履约中/已关闭/已作废不在其中）。 */
const PENDING_REVIEW_STAGE_CODES = [
    "awaiting_confirm",
    "awaiting_sales",
    "awaiting_sales_lead",
    "awaiting_ops",
]

export function isPendingReviewStage(code: string) {
    return PENDING_REVIEW_STAGE_CODES.includes(code)
}

/** 当前阶段责任人展示文案：有派发待办时按角色+姓名；驳回/低毛利待处理归销售本人。 */
export function stageOwnerDisplay(order: SalesOrderListItem): string {
    const ownerRole = order.primaryStatus.ownerRole
    if (ownerRole) {
        const roleLabel = STAGE_OWNER_ROLE_LABEL[ownerRole] ?? ownerRole
        return `${roleLabel} · ${order.primaryStatus.ownerUserName ?? "待认领"}`
    }
    if (order.primaryStatus.code === "awaiting_sales") {
        return `销售 · ${order.ownerName}`
    }
    return "待分配"
}

/** 当前阶段预计完成时限；未设置时返回 `undefined`（面板自动显示"未设置"）。 */
export function stageDueDisplay(
    order: SalesOrderListItem,
): DisplayTime | undefined {
    const dueAt = order.primaryStatus.dueAt
    if (!dueAt) return undefined
    const iso = new Date(dueAt * 1000).toISOString()
    return { dateTime: iso, label: formatDateTime(iso, "full") }
}

/** 创建来源文案（MALL = 商城入口；ERP = 本系统入口）。 */
export const ORIGIN_LABEL: Record<SalesOrderListItem["originSystem"], string> =
    {
        erp: "创建于 ERP",
        mall: "创建于商城",
    }

/** 采购驳回原因码文案（后端 `ProcurementRejectReasonCode` + 历史兼容）。 */
export const PROCUREMENT_REJECT_REASON_LABEL: Record<string, string> = {
    CANNOT_FULFILL: "无法履约",
    COST_INCREASE: "成本上涨",
    DELIVERY_NOT_MET: "交期不满足",
    QUALIFICATION_EXPIRED: "资质失效",
    OTHER: "其他",
    MARGIN_TOO_LOW: "预计毛利过低",
    COST_TOO_HIGH: "采购成本过高",
    ITEM_UNAVAILABLE: "商品/服务无法采购",
    TERMS_UNCLEAR: "商业条件不清晰",
}

/** 卡券审批任务类型文案。 */
export const CARD_APPROVAL_TYPE_LABEL: Record<
    CardSalesApproval["workItemType"],
    string
> = {
    CARD_SALES_MANAGER_APPROVAL: "卡券销售领导审批",
    CARD_SALES_OPERATION_APPROVAL: "卡券销售运营审批",
}
