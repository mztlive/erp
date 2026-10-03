import { apiGet } from "@/lib/api"
import type {
    BackendPage,
    BackendStockAdjustmentDetail,
    BackendStockBalance,
} from "./dto"
import { directionFrontend, reasonTypeFrontend } from "./display"
import { mapAdjustmentApproval, mapBalance } from "./mappers"
import type {
    AdjustmentReasonType,
    StockAdjustmentApprovalView,
} from "../types"

export type EditableAdjustment = {
    id: string
    adjustmentNo: string
    warehouseName: string
    reasonType: AdjustmentReasonType
    note: string
    occurredAt: string
    approval: StockAdjustmentApprovalView
    lines: {
        lineId: string
        skuCode: string
        skuName: string
        quantity: string
        direction: "increase" | "decrease"
        balanceId: string
        balanceVersion: string
        onHand: string
        available: string
    }[]
}

function localDateTime(seconds: number): string {
    const value = new Date(seconds * 1000)
    const pad = (part: number) => String(part).padStart(2, "0")
    return `${value.getFullYear()}-${pad(value.getMonth() + 1)}-${pad(value.getDate())}T${pad(value.getHours())}:${pad(value.getMinutes())}`
}

/** 读取原草稿的全部行与真实余额版本，禁止重建单据或用合成 ID 提交。 */
export async function fetchEditableAdjustment(
    id: string,
): Promise<EditableAdjustment> {
    const detail = await apiGet<BackendStockAdjustmentDetail>(
        `/admin/stock-adjustments/${encodeURIComponent(id)}`,
    )
    const approval = mapAdjustmentApproval(detail.approval)
    if (
        detail.adjustment.status !== "DRAFT" ||
        !approval.submitCommand ||
        !approval.allowedActions.includes("SUBMIT")
    ) {
        throw new Error("当前调整单不能编辑，请刷新详情后重试。")
    }
    if (!detail.lines.length)
        throw new Error("调整明细缺失，请刷新详情后重试。")
    const skuIds = [...new Set(detail.lines.map((line) => line.sku_id))]
    const balances = await Promise.all(
        skuIds.map(async (skuId) => {
            const page = await apiGet<BackendPage<BackendStockBalance>>(
                "/admin/stock-balances",
                {
                    warehouse_id: detail.adjustment.warehouse_id,
                    sku_id: skuId,
                    page: 1,
                    page_size: 2,
                },
            )
            const matches = page.items.filter(
                (balance) =>
                    balance.warehouse_id === detail.adjustment.warehouse_id &&
                    balance.sku_id === skuId,
            )
            if (matches.length !== 1 || page.total !== 1) {
                throw new Error("无法确认原单对应的库存余额，请刷新后重试。")
            }
            return mapBalance(matches[0])
        }),
    )
    const bySku = new Map(balances.map((balance) => [balance.skuId, balance]))
    return {
        id: detail.adjustment.id,
        adjustmentNo: detail.adjustment.adjustment_no,
        warehouseName: balances[0].warehouseName,
        reasonType: reasonTypeFrontend(
            detail.adjustment.reason_type,
        ) as AdjustmentReasonType,
        note: detail.adjustment.note ?? "",
        occurredAt: localDateTime(
            detail.adjustment.occurred_at ?? detail.adjustment.created_at,
        ),
        approval,
        lines: detail.lines.map((line) => {
            const balance = bySku.get(line.sku_id)!
            return {
                lineId: line.id,
                skuCode: balance.skuCode,
                skuName: balance.skuName,
                quantity: line.quantity,
                direction: directionFrontend(line.direction),
                balanceId: balance.balanceId,
                balanceVersion: balance.lockVersion,
                onHand: balance.onHandQuantity,
                available: balance.availableQuantity,
            }
        }),
    }
}
