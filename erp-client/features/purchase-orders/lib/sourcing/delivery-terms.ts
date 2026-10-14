import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"

import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    type DeliveryTerm,
    type SupplyPriceBasis,
} from "../../types"
import { findSourcingOption } from "./options"
import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "./types"

const TERM_LABEL: Record<SupplyPriceBasis, Record<DeliveryTerm, string>> = {
    DROPSHIP: {
        FREIGHT_INCLUDED: "配送",
        FREIGHT_EXTRA: "运费另计",
        NOT_DELIVERABLE: "不配送",
    },
    BULK: {
        FREIGHT_INCLUDED: "包邮",
        FREIGHT_EXTRA: "运费另计",
        NOT_DELIVERABLE: "不配送",
    },
}

/** 方案名称：现有库存、代发直发、集采直发、集采入仓或固有履约方式。 */
export function sourcingRouteLabel(option: SourcingSupplierOption): string {
    if (option.sourceType === "EXISTING_STOCK") return "现有库存"
    if (option.fulfillmentResponsibility === "SUPPLIER_DIRECT")
        return option.priceBasis === "BULK" ? "集采直发" : "代发直发"
    if (option.fulfillmentResponsibility === "WAREHOUSE") return "集采入仓"
    return FULFILLMENT_RESPONSIBILITY_LABEL[option.fulfillmentResponsibility]
}

/** 判定的中文名称；代发的包邮显示为「配送」。 */
export function deliveryTermLabel(
    option: SourcingSupplierOption,
    term: DeliveryTerm,
): string {
    return TERM_LABEL[option.priceBasis ?? "DROPSHIP"][term]
}

function isDirect(option: SourcingSupplierOption | undefined) {
    return (
        option?.sourceType === "PURCHASE" &&
        option.fulfillmentResponsibility === "SUPPLIER_DIRECT"
    )
}

/** 直发方案对一个收货地区的判定；非直发方案或缺少判定时为空。 */
export function addressDeliveryTerm(
    option: SourcingSupplierOption | undefined,
    regionCode: string,
): DeliveryTerm | undefined {
    if (!isDirect(option)) return undefined
    return option?.delivery?.regionTerms[regionCode]
}

/** 集采入仓方案对一个目标仓的判定；非入仓方案或仓库缺少地区时为空。 */
export function warehouseDeliveryTerm(
    option: SourcingSupplierOption | undefined,
    warehouseId: string,
) {
    if (
        option?.sourceType !== "PURCHASE" ||
        option.fulfillmentResponsibility !== "WAREHOUSE" ||
        !warehouseId
    )
        return undefined
    return option.delivery?.warehouseTerms.find(
        (item) => item.warehouseId === warehouseId,
    )
}

function salesRegions(product: SourcingProductLine): string[] {
    return [
        ...new Set(product.deliveryAllocations.map((row) => row.region_code)),
    ]
}

/** 直发方案逐地区判定计数。 */
function directTermCounts(
    option: SourcingSupplierOption,
    product: SourcingProductLine,
) {
    const counts: Record<DeliveryTerm, number> = {
        FREIGHT_INCLUDED: 0,
        FREIGHT_EXTRA: 0,
        NOT_DELIVERABLE: 0,
    }
    for (const region of salesRegions(product)) {
        counts[addressDeliveryTerm(option, region) ?? "FREIGHT_INCLUDED"] += 1
    }
    return counts
}

/** 方案的配送摘要，例如「2 个地址运费另计、1 个地址不配送」；无地址或非直发时为空。 */
export function deliverySummary(
    option: SourcingSupplierOption,
    product: SourcingProductLine,
): string {
    if (!isDirect(option) || !product.deliveryAllocations.length) return ""
    const counts = directTermCounts(option, product)
    const parts = [
        counts.FREIGHT_EXTRA
            ? `${counts.FREIGHT_EXTRA} 个地址${deliveryTermLabel(option, "FREIGHT_EXTRA")}`
            : "",
        counts.NOT_DELIVERABLE
            ? `${counts.NOT_DELIVERABLE} 个地址${deliveryTermLabel(option, "NOT_DELIVERABLE")}`
            : "",
    ].filter(Boolean)
    return parts.length
        ? parts.join("、")
        : `全部地址${deliveryTermLabel(option, "FREIGHT_INCLUDED")}`
}

/** 直发方案的全部收货地区都不配送。 */
export function directOptionUndeliverable(
    option: SourcingSupplierOption,
    product: SourcingProductLine,
): boolean {
    if (!isDirect(option) || !product.deliveryAllocations.length) return false
    const counts = directTermCounts(option, product)
    return counts.FREIGHT_INCLUDED + counts.FREIGHT_EXTRA === 0
}

/** 直发方案存在运费另计的收货地区。 */
export function directOptionHasFreightExtra(
    option: SourcingSupplierOption,
    product: SourcingProductLine,
): boolean {
    if (!isDirect(option) || !product.deliveryAllocations.length) return false
    return directTermCounts(option, product).FREIGHT_EXTRA > 0
}

/** 收货地址是否可分配给本方案。 */
export function addressDeliverable(
    option: SourcingSupplierOption | undefined,
    regionCode: string,
): boolean {
    return addressDeliveryTerm(option, regionCode) !== "NOT_DELIVERABLE"
}

/**
 * 配送范围与起订量校验：不配送地址不得分配数量，不配送仓库不得作为目标仓，
 * 集采直发向同一供给的数量合计不少于集采起订量。
 */
export function deliveryJudgementErrors(
    order: SourcingSalesOrder,
    lines: readonly SourcingLineInput[],
    allocationsOf: (
        product: SourcingProductLine,
        line: SourcingLineInput,
    ) => readonly { region_code: string; quantity: string }[],
): Array<[number, string]> {
    const errors: Array<[number, string]> = []
    const bulkTotals = new Map<
        string,
        {
            minimum: string
            quantities: string[]
            indexes: number[]
            name: string
        }
    >()
    lines.forEach((line, index) => {
        if (!line.selected) return
        const product = order.lines.find(
            (candidate) => candidate.salesOrderLineId === line.salesOrderLineId,
        )
        const option = findSourcingOption(product, line.basisId)
        if (!product || !option || option.sourceType !== "PURCHASE") return
        for (const row of allocationsOf(product, line)) {
            if (
                !addressDeliverable(option, row.region_code) &&
                compareDecimalSafe(row.quantity, "0") > 0
            )
                errors.push([
                    index,
                    `${product.itemName}：有收货地址不在该方案配送范围内，请改选其他方案`,
                ])
        }
        const warehouse = warehouseDeliveryTerm(
            option,
            line.targetWarehouseId ?? "",
        )
        if (warehouse?.term === "NOT_DELIVERABLE")
            errors.push([
                index,
                `${product.itemName}：目标仓所在地区${warehouse.regionName}不在集采配送范围内`,
            ])
        const minimum = option.delivery?.bulkMinimumOrderQuantity
        const offeringId = option.delivery?.supplierOfferingId
        if (
            minimum &&
            offeringId &&
            option.priceBasis === "BULK" &&
            isDirect(option)
        ) {
            const entry = bulkTotals.get(offeringId) ?? {
                minimum,
                quantities: [],
                indexes: [],
                name: product.itemName,
            }
            entry.quantities.push(line.quantity || "0")
            entry.indexes.push(index)
            bulkTotals.set(offeringId, entry)
        }
    })
    for (const entry of bulkTotals.values()) {
        try {
            const total = sumFixed(entry.quantities, {
                maxScale: 6,
                outputScale: 6,
            })
            if (compareDecimal(total, entry.minimum, 6) < 0)
                for (const index of entry.indexes)
                    errors.push([
                        index,
                        `${entry.name}：集采直发数量合计须不少于集采起订量 ${entry.minimum}`,
                    ])
        } catch {
            // 数量格式错误由数量校验单独提示。
        }
    }
    return errors
}

function compareDecimalSafe(left: string, right: string): number {
    try {
        return compareDecimal(left || "0", right, 6)
    } catch {
        return 0
    }
}
