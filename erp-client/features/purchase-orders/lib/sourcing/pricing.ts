import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { salesLinePriceComparison } from "@/lib/sales-line-prices"
import type { PurchaseOrderCreateFormApi } from "../purchase-order-create-form-types"
import {
    buildFillDirectShortfallPatches,
    isPositiveQuantity,
} from "./address-grouping"
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

/** 找出候选采购方案中的最低含税单价（现有库存按库存核算不计采购单价）。 */
export function findLowestPurchaseCost(
    options: readonly SourcingSupplierOption[],
): string | undefined {
    let lowest: string | undefined
    for (const option of options) {
        if (option.sourceType !== "PURCHASE") continue
        const cost = option.unitCostGross?.trim()
        if (!cost) continue
        try {
            if (!lowest || compareDecimal(cost, lowest, 4) < 0) {
                lowest = compactFixed(cost)
            }
        } catch {
            // 跳过非法金额
        }
    }
    return lowest
}

/** 计算某个采购单价相对全场最低采购价的价差。 */
export function priceDeltaFromLowest(
    unitCostGross: string | undefined,
    lowestCost: string | undefined,
): { isLowest: boolean; delta: string } | null {
    const cost = unitCostGross?.trim()
    if (!cost || !lowestCost) return null
    try {
        const cmp = compareDecimal(cost, lowestCost, 4)
        if (cmp <= 0) return { isLowest: true, delta: "0" }
        const diff = subtractFixed(cost, lowestCost, {
            maxScale: 4,
            outputScale: 4,
        })
        return { isLowest: false, delta: compactFixed(diff) }
    } catch {
        return null
    }
}

export type WeightedProcurementSummary = Readonly<{
    /** 参与采购来源的加权含税均价（按分配数量加权）；无有效采购量时为空。 */
    weightedUnitCostGross?: string
    /** 参与采购来源的预估含税总采购额。 */
    totalProcurementCost?: string
    /** 相对销售含税单价的综合毛利测算。 */
    margin: MarginComparison | null
    /** 参与本次分配的采购来源数量。 */
    purchaseSourceCount: number
    /** 参与本次分配的现有库存来源数量。 */
    stockSourceCount: number
    /** 是否存在任一价格倒挂来源。 */
    hasInvertedSource: boolean
    /** 参与采购来源是否存在多个不同单价。 */
    hasMixedPrices: boolean
}>

/** 计算商品多源组合下的真实加权平均采购价、预估采购总额与综合毛利。 */
export function summarizeWeightedProcurementCost(
    row: SourcingEditorRow,
): WeightedProcurementSummary {
    const selected = row.allocations.filter(({ line }) => line.selected)
    const relevant = selected.length > 0 ? selected : row.allocations
    const salesUnitPriceGross =
        row.product.salesPrices?.unit_price_gross?.trim() || undefined

    let purchaseSourceCount = 0
    let stockSourceCount = 0
    let hasInvertedSource = false
    const distinctPrices = new Set<string>()
    const purchaseAmounts: string[] = []
    const purchaseQuantities: string[] = []

    for (const { line } of relevant) {
        const option = findSourcingOption(row.product, line.basisId)
        if (!option) continue
        if (option.sourceType === "EXISTING_STOCK") {
            stockSourceCount += 1
            continue
        }
        purchaseSourceCount += 1
        const cost = option.unitCostGross?.trim()
        if (!cost) continue
        try {
            distinctPrices.add(compactFixed(cost))
        } catch {
            distinctPrices.add(cost)
        }
        const singleMargin = comparePurchaseCostToSalesPrice(
            cost,
            salesUnitPriceGross,
        )
        if (singleMargin?.inverted) hasInvertedSource = true

        const qty = line.quantity?.trim()
        if (qty && isPositiveQuantity(qty)) {
            try {
                const amount = multiplyFixed(cost, compactFixed(qty), {
                    leftMaxScale: 4,
                    rightMaxScale: 6,
                    outputScale: 4,
                })
                purchaseAmounts.push(amount)
                purchaseQuantities.push(compactFixed(qty))
            } catch {
                // 跳过未填完的非法数量
            }
        }
    }

    let weightedUnitCostGross: string | undefined
    let totalProcurementCost: string | undefined
    if (purchaseAmounts.length > 0 && purchaseQuantities.length > 0) {
        try {
            const totalCost = sumFixed(purchaseAmounts, {
                maxScale: 4,
                outputScale: 2,
            })
            const totalQty = sumFixed(purchaseQuantities, {
                maxScale: 6,
                outputScale: 6,
            })
            totalProcurementCost = compactFixed(totalCost)
            if (compareDecimal(totalQty, "0", 6) > 0) {
                const rawTotalCost = sumFixed(purchaseAmounts, {
                    maxScale: 4,
                    outputScale: 4,
                })
                weightedUnitCostGross = compactFixed(
                    divideFixed(rawTotalCost, totalQty, {
                        numeratorMaxScale: 4,
                        denominatorMaxScale: 6,
                        outputScale: 2,
                    }),
                )
            }
        } catch {
            // 保留空值交由回退逻辑展示
        }
    } else if (distinctPrices.size === 1) {
        weightedUnitCostGross = [...distinctPrices][0]
    }

    const margin = comparePurchaseCostToSalesPrice(
        weightedUnitCostGross,
        salesUnitPriceGross,
    )

    return {
        weightedUnitCostGross,
        totalProcurementCost,
        margin,
        purchaseSourceCount,
        stockSourceCount,
        hasInvertedSource,
        hasMixedPrices: distinctPrices.size > 1,
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

/** 统一应用来源切换联动：更新 basisId、交期、目标仓、默认数量与地址同步。 */
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

    const otherSelectedQty = lines
        .filter(
            (entry, entryIndex) =>
                entryIndex !== index &&
                entry.salesOrderLineId === product.salesOrderLineId &&
                entry.selected &&
                isPositiveQuantity(entry.quantity || "0"),
        )
        .map((entry) => compactFixed(entry.quantity.trim()))
    let residualDemand = product.remainingQuantity
    if (otherSelectedQty.length > 0) {
        try {
            const used = sumFixed(otherSelectedQty, {
                maxScale: 6,
                outputScale: 6,
            })
            const diff = subtractFixed(product.remainingQuantity, used, {
                maxScale: 6,
                outputScale: 6,
            })
            residualDemand =
                compareDecimal(diff, "0", 6) > 0 ? compactFixed(diff) : "0"
        } catch {
            residualDemand = product.remainingQuantity
        }
    }

    const isDirectWithAddresses =
        option.sourceType === "PURCHASE" &&
        option.fulfillmentResponsibility === "SUPPLIER_DIRECT" &&
        product.deliveryAllocations.length > 0

    const currentQty = current.quantity?.trim() ?? ""
    let nextQty = currentQty
    if (!currentQty || currentQty === product.remainingQuantity) {
        if (otherSelectedQty.length > 0) {
            if (isPositiveQuantity(residualDemand)) {
                try {
                    nextQty =
                        compareDecimal(
                            residualDemand,
                            option.maxCreateQuantity,
                            6,
                        ) <= 0
                            ? residualDemand
                            : option.maxCreateQuantity
                } catch {
                    nextQty = option.maxCreateQuantity
                }
            } else {
                nextQty = isDirectWithAddresses ? "0" : ""
            }
        } else {
            nextQty = option.maxCreateQuantity
        }
    } else {
        try {
            if (compareDecimal(currentQty, option.maxCreateQuantity, 6) > 0) {
                nextQty = option.maxCreateQuantity
            }
        } catch {
            nextQty = option.maxCreateQuantity
        }
    }

    const needsWarehouse =
        option.sourceType === "PURCHASE" &&
        option.fulfillmentResponsibility === "WAREHOUSE"

    const updatedLine = {
        ...current,
        basisId: option.basisId,
        quantity: nextQty,
        expectedDeliveryDate: option.expectedDeliveryDate,
        targetWarehouseId: needsWarehouse ? current.targetWarehouseId : "",
        targetWarehouseName: needsWarehouse ? current.targetWarehouseName : "",
        deliveryAllocations:
            previousBasisId !== option.basisId
                ? undefined
                : current.deliveryAllocations,
    }

    // 切到带地址直发且并非单源全量默认覆盖时，自动按目标量分配可用地址，避免地址合计与数量脱节
    if (
        previousBasisId !== option.basisId &&
        isDirectWithAddresses &&
        (otherSelectedQty.length > 0 ||
            (nextQty !== "" && nextQty !== product.remainingQuantity))
    ) {
        const probeLines = lines.map((entry, entryIndex) =>
            entryIndex === index
                ? {
                      ...updatedLine,
                      deliveryAllocations: [],
                      quantity: "0",
                  }
                : entry,
        )
        if (isPositiveQuantity(nextQty)) {
            const filled = buildFillDirectShortfallPatches(
                product,
                probeLines,
                current.rowKey,
                nextQty,
            )
            const targetPatch = filled.patches.find(
                (patch) => patch.index === index,
            )
            if (targetPatch) {
                form.setFieldValue(`lines[${index}]`, targetPatch.nextLine)
                return
            }
        }
        form.setFieldValue(`lines[${index}]`, {
            ...updatedLine,
            deliveryAllocations: [],
            quantity: "0",
        })
        return
    }

    form.setFieldValue(`lines[${index}].basisId`, updatedLine.basisId)
    form.setFieldValue(`lines[${index}].quantity`, updatedLine.quantity)
    form.setFieldValue(
        `lines[${index}].expectedDeliveryDate`,
        updatedLine.expectedDeliveryDate,
    )
    if (!needsWarehouse) {
        form.setFieldValue(`lines[${index}].targetWarehouseId`, "")
        form.setFieldValue(`lines[${index}].targetWarehouseName`, "")
    }
    if (previousBasisId !== option.basisId) {
        form.setFieldValue(`lines[${index}].deliveryAllocations`, undefined)
    }
}
