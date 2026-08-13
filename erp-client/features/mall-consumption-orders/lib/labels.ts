/**
 * 纯展示标签拼装：列表单元格与预览摘要用的中文文案，不依赖 React。
 */

import type {
    MallConsumptionOrderRow,
    MallConsumptionOrderView,
    SupplierFulfillmentStatus,
} from "@/features/mall-consumption-orders/types"
import {
    COST_BASIS_LABEL,
    DATA_SOURCE_LABEL,
    FACT_TYPE_LABEL,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"

export function paymentCompositionLabel(row: MallConsumptionOrderRow) {
    const { cardAmount, wechatAmount, sourceCount } = row.paymentComposition
    const card = Number(cardAmount) > 0
    const wx = Number(wechatAmount) > 0
    if (card && wx) {
        return `组合 · 卡券 ¥${cardAmount} / 微信 ¥${wechatAmount}`
    }
    if (card) return `卡券 ¥${cardAmount}`
    if (wx) return `微信 ¥${wechatAmount}`
    return `${sourceCount} 来源`
}

export function factSummaryLabel(row: MallConsumptionOrderRow) {
    return row.factSummary
        .map(
            (f) =>
                `${FACT_TYPE_LABEL[f.factType]}${f.count > 1 ? `×${f.count}` : ""}`,
        )
        .join(" · ")
}

export function costBasisLabel(row: MallConsumptionOrderRow) {
    return row.costBasisBreakdown
        .map((b) => {
            const basisLabel = COST_BASIS_LABEL[b.basis] ?? b.basis
            return `${basisLabel}${b.lineCount > 1 ? `×${b.lineCount}` : ""}`
        })
        .join(" / ")
}

export function supplierSummaryLabel(row: MallConsumptionOrderRow) {
    const s = row.supplierOrderSummary
    if (s.total === 0) {
        if (row.fulfillmentChain === "LEGACY_MANUAL") return "原人工 · 无子订单"
        return "尚未生成子订单"
    }
    const statusText = s.statuses
        .map(
            (st) =>
                SUPPLIER_STATUS_LABEL[st as SupplierFulfillmentStatus] ?? st,
        )
        .join("/")
    return `${s.total} 单 · ${statusText}${s.hasException ? " · 异常" : ""}`
}

export function previewDataSourceLabel(view: MallConsumptionOrderView): string {
    if (view.facts.length === 0) return "—"
    const kinds = Array.from(new Set(view.facts.map((f) => f.dataSource)))
    if (kinds.length === 1) return DATA_SOURCE_LABEL[kinds[0]]
    return DATA_SOURCE_LABEL.MIXED
}
