import { compareDecimal } from "@/lib/fixed-decimal"
import { findSourcingOption } from "./options"
import { formatQuantityUnits, quantityUnits } from "./quantity"
import {
    buildDefaultSourcingLines,
    optionCoversRemaining,
} from "./recommendation"
import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
} from "./types"

/** 复用当前参与范围的推荐结果；仅为仍采用推荐来源的采购行说明依据。 */
export function sourcingRecommendationReason(
    order: SourcingSalesOrder,
    product: SourcingProductLine,
    line: SourcingLineInput,
    lines: readonly SourcingLineInput[],
): { title: string; detail: string } | undefined {
    const option = findSourcingOption(product, line.basisId)
    if (!line.selected || option?.sourceType !== "PURCHASE") return undefined
    const included = new Set(
        lines
            .filter((entry) => entry.selected)
            .map((entry) => entry.salesOrderLineId),
    )
    const recommended = buildDefaultSourcingLines({
        ...order,
        lines: order.lines.filter((entry) =>
            included.has(entry.salesOrderLineId),
        ),
    }).filter((entry) => entry.salesOrderLineId === product.salesOrderLineId)
    if (!recommended.some((entry) => entry.basisId === option.basisId))
        return undefined

    const stockQuantity = recommended.reduce(
        (sum, entry) =>
            findSourcingOption(product, entry.basisId)?.sourceType ===
            "EXISTING_STOCK"
                ? sum + quantityUnits(entry.quantity)
                : sum,
        BigInt(0),
    )
    const remaining = formatQuantityUnits(
        quantityUnits(product.remainingQuantity) - stockQuantity,
    )
    const purchases = product.options.filter(
        (entry) => entry.sourceType === "PURCHASE",
    )
    if (purchases.length === 1) {
        return {
            title: "当前唯一可用采购方案",
            detail: "当前数据中仅有这一项可用采购方案，系统自动带入。",
        }
    }

    const direct = option.fulfillmentResponsibility === "SUPPLIER_DIRECT"
    const covers = optionCoversRemaining(option, remaining)
    const peers = purchases.filter(
        (entry) =>
            (entry.fulfillmentResponsibility === "SUPPLIER_DIRECT") ===
                direct && optionCoversRemaining(entry, remaining) === covers,
    )
    const scope = direct ? "优先选择供应商直发" : "当前无可用直发方案"
    const coverage = covers ? "可全量覆盖采购缺口" : "仅能部分覆盖采购缺口"
    const basis = `${scope}；${coverage}。`
    if (peers.length === 1) {
        return {
            title: direct ? "直发与供货数量优先" : "供货数量优先",
            detail: `${basis}当前仅这一项方案符合上述优先条件。`,
        }
    }

    try {
        const costs = peers.map((entry) =>
            compareDecimal(entry.unitCostGross, option.unitCostGross, 4),
        )
        if (costs.some((cost) => cost < 0)) return undefined
        const tied = costs.filter((cost) => cost === 0).length > 1
        const supplierCount = new Set(peers.map((entry) => entry.supplierId))
            .size
        return {
            title: tied ? "含税单价并列最低" : "含税单价最低",
            detail: `${basis}在同等条件的 ${supplierCount} 家供应商、${peers.length} 项可用方案中${tied ? "并列" : ""}最低。${tied ? "同价时依次比较交付日、可供量和供应商名称。" : ""}`,
        }
    } catch {
        return undefined
    }
}
