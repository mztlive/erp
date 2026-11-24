import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { salesLinePriceComparison } from "@/lib/sales-line-prices"
import type { PurchaseOrderCreateFormApi } from "../purchase-order-create-form-types"
import { findSourcingOption } from "./options"
import type { SourcingEditorRow } from "./editor-rows"
import type {
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "./types"

export type MarginComparison = Readonly<{
    /** 采购含税单价 > 销售含税单价（负毛利/倒挂）。 */
    inverted: boolean
    /** 售价 - 采价（可为负），保留 2–4 位定点字符串。 */
    spread: string
    /** 绝对差额，便于文案展示「高于售价 ¥X」。 */
    absSpread: string
    /** 毛利率百分比文本（如 `+13.5%` / `-13.5%`）；无售价或售价为 0 时为空。 */
    marginRateText: string | null
}>

/** 用定点十进制比较采购含税单价与销售含税单价，严禁经过 JS Number。 */
export function comparePurchaseCostToSalesPrice(
    unitCostGross: string | null | undefined,
    salesUnitPriceGross: string | null | undefined,
): MarginComparison | null {
    const cost = unitCostGross?.trim()
    const sales = salesUnitPriceGross?.trim()
    if (!cost || !sales) return null
    try {
        const cmp = compareDecimal(cost, sales, 4)
        const inverted = cmp > 0
        const spread = compactFixed(
            subtractFixed(sales, cost, { maxScale: 4, outputScale: 4 }),
        )
        const absSpread = compactFixed(
            subtractFixed(inverted ? cost : sales, inverted ? sales : cost, {
                maxScale: 4,
                outputScale: 4,
            }),
        )
        let marginRateText: string | null = null
        if (compareDecimal(sales, "0", 4) > 0) {
            const pct = divideFixed(
                multiplyFixed(absSpread, "100", {
                    leftMaxScale: 4,
                    rightMaxScale: 0,
                    outputScale: 4,
                }),
                sales,
                {
                    numeratorMaxScale: 4,
                    denominatorMaxScale: 4,
                    outputScale: 1,
                },
            )
            const compactPct = compactFixed(pct)
            marginRateText =
                cmp === 0
                    ? "0%"
                    : inverted
                      ? `-${compactPct}%`
                      : `+${compactPct}%`
        }
        return { inverted, spread, absSpread, marginRateText }
    } catch {
        return null
    }
}

export type RowPricingSummary = Readonly<{
    /** 单一采购价、库存核算、多个价格或未选。 */
    kind: "purchase" | "stock" | "multiple" | "empty"
    unitCostGross?: string
    margin: MarginComparison | null
    /** 该行是否有任一参与采购来源出现价格倒挂。 */
    hasInvertedSource: boolean
    /** 倒挂来源中的最高采购含税单价。 */
    highestInvertedCost?: string
    salesUnitPriceGross?: string
    supplyReferenceGross?: string
    salesWarnings: readonly string[]
}>

/** 计算商品主表行与预览风控共用的价格/倒挂摘要。 */
export function summarizeRowPricing(row: SourcingEditorRow): RowPricingSummary {
    const active = row.allocations.filter(({ line }) => line.selected)
    const relevant = active.length > 0 ? active : row.allocations
    const options = relevant
        .map(({ line }) => findSourcingOption(row.product, line.basisId))
        .filter((option): option is SourcingSupplierOption => Boolean(option))

    const salesUnitPriceGross =
        row.product.salesPrices?.unit_price_gross?.trim() || undefined
    const supplyReferenceGross =
        row.product.salesPrices?.procurement_supply_price_gross?.trim() ||
        undefined
    const salesWarnings = row.product.salesPrices
        ? salesLinePriceComparison(row.product.salesPrices).warnings
        : []

    let highestInvertedCost: string | undefined
    for (const option of options) {
        if (option.sourceType !== "PURCHASE") continue
        const margin = comparePurchaseCostToSalesPrice(
            option.unitCostGross,
            salesUnitPriceGross,
        )
        if (!margin?.inverted) continue
        if (
            !highestInvertedCost ||
            compareDecimal(option.unitCostGross, highestInvertedCost, 4) > 0
        ) {
            highestInvertedCost = option.unitCostGross
        }
    }

    if (options.length === 0) {
        return {
            kind: "empty",
            margin: null,
            hasInvertedSource: false,
            salesUnitPriceGross,
            supplyReferenceGross,
            salesWarnings,
        }
    }

    const keys = [
        ...new Set(
            options.map((option) =>
                option.sourceType === "EXISTING_STOCK"
                    ? "__STOCK__"
                    : option.unitCostGross,
            ),
        ),
    ]
    if (keys.length > 1) {
        return {
            kind: "multiple",
            margin: highestInvertedCost
                ? comparePurchaseCostToSalesPrice(
                      highestInvertedCost,
                      salesUnitPriceGross,
                  )
                : null,
            hasInvertedSource: Boolean(highestInvertedCost),
            highestInvertedCost,
            salesUnitPriceGross,
            supplyReferenceGross,
            salesWarnings,
        }
    }

    const single = keys[0]
    if (single === "__STOCK__") {
        return {
            kind: "stock",
            margin: null,
            hasInvertedSource: false,
            salesUnitPriceGross,
            supplyReferenceGross,
            salesWarnings,
        }
    }

    const margin = comparePurchaseCostToSalesPrice(single, salesUnitPriceGross)
    return {
        kind: "purchase",
        unitCostGross: single,
        margin,
        hasInvertedSource: Boolean(margin?.inverted),
        highestInvertedCost,
        salesUnitPriceGross,
        supplyReferenceGross,
        salesWarnings,
    }
}

/** 计算整张销售单最早的承诺最晚交付日，用于页头紧凑展示。 */
export function earliestOrderDeliveryDeadline(
    order: SourcingSalesOrder | undefined,
): string | undefined {
    if (!order) return undefined
    const dates = order.lines
        .map((line) => line.deliveryDeadline?.trim())
        .filter((value): value is string => Boolean(value))
        .sort()
    return dates[0]
}

/** 统一应用来源切换联动：更新 basisId、交期、目标仓、默认数量与地址重置。 */
export function applySourcingOptionToLine(
    form: PurchaseOrderCreateFormApi,
    product: SourcingProductLine,
    index: number,
    option: SourcingSupplierOption,
): void {
    const lines = form.state.values.lines
    const current = lines[index]
    if (!current) return
    const previousBasisId = current.basisId

    form.setFieldValue(`lines[${index}].basisId`, option.basisId)

    const currentQty = current.quantity?.trim() ?? ""
    if (!currentQty || currentQty === product.remainingQuantity) {
        form.setFieldValue(`lines[${index}].quantity`, option.maxCreateQuantity)
    } else {
        try {
            if (compareDecimal(currentQty, option.maxCreateQuantity, 6) > 0) {
                form.setFieldValue(
                    `lines[${index}].quantity`,
                    option.maxCreateQuantity,
                )
            }
        } catch {
            form.setFieldValue(
                `lines[${index}].quantity`,
                option.maxCreateQuantity,
            )
        }
    }

    form.setFieldValue(
        `lines[${index}].expectedDeliveryDate`,
        option.expectedDeliveryDate,
    )

    if (
        option.sourceType !== "PURCHASE" ||
        option.fulfillmentResponsibility !== "WAREHOUSE"
    ) {
        form.setFieldValue(`lines[${index}].targetWarehouseId`, "")
        form.setFieldValue(`lines[${index}].targetWarehouseName`, "")
    }

    if (previousBasisId !== option.basisId) {
        form.setFieldValue(`lines[${index}].deliveryAllocations`, undefined)
    }
}
