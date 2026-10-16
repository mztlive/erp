import type {
    PurchaseChangeDraft,
    PurchaseChangeDraftLine,
} from "../api/purchase-change-draft"
import type {
    PurchaseDeliveryLineOption,
    PurchaseQuantityBound,
} from "../api/purchase-delivery-options"
import { PURCHASE_CHANGE_PLACEHOLDER_REASON } from "../api/purchase-order-commands"
import type { PurchaseDeliveryAllocation } from "../types"
import {
    editableDeliveryRows,
    positiveDeliveryRows,
} from "./delivery-adjustment"
import {
    compactFixed,
    compareDecimal,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

export type PurchaseChangeDeliveryOptions = ReadonlyMap<
    string,
    PurchaseDeliveryLineOption
>

export type PurchaseChangeLineValues = {
    /** 冻结行主键，仅作表单身份。 */
    lineKey: string
    /** 改单目标中的完整原行，提交时保留全部冻结关联。 */
    source: PurchaseChangeDraftLine
    quantity: string
    unitCostGross: string
    inputTaxRate: string
    /** 物流费用行含税金额。 */
    grossAmount: string
    expectedDeliveryDate: string
    deliveryRows: PurchaseDeliveryAllocation[]
}

export type PurchaseChangeDraftValues = {
    reason: string
    paymentTermCode: string
    lines: PurchaseChangeLineValues[]
}

/** 商品行对应的交付安排与数量范围；仍在销售当前版本中的商品行才有。 */
export function purchaseChangeLineOption(
    line: PurchaseChangeDraftLine,
    options: PurchaseChangeDeliveryOptions,
) {
    if (line.line_type !== "ITEM_SERVICE" || !line.sales_order_line_id)
        return undefined
    return options.get(line.sales_order_line_id)
}

/** 由服务端草稿恢复表单；占位原因显示为空，要求重新填写。 */
export function purchaseChangeDraftValues(
    draft: PurchaseChangeDraft,
    options: PurchaseChangeDeliveryOptions,
): PurchaseChangeDraftValues {
    return {
        reason:
            draft.reason === PURCHASE_CHANGE_PLACEHOLDER_REASON
                ? ""
                : draft.reason,
        paymentTermCode: draft.payment_term_code,
        lines: draft.lines.map((line, index) => {
            const option = purchaseChangeLineOption(line, options)
            const current = line.delivery_allocations ?? []
            return {
                lineKey: draft.line_keys[index],
                source: line,
                quantity: compactFixed(line.quantity ?? "0"),
                unitCostGross: line.unit_cost_gross ?? "",
                inputTaxRate: compactFixed(line.input_tax_rate ?? "0"),
                grossAmount: line.gross_amount ?? "",
                expectedDeliveryDate: line.expected_delivery_date ?? "",
                deliveryRows: option?.requiresAddresses
                    ? editableDeliveryRows(option, current)
                    : current,
            }
        }),
    }
}

/** 当前生效版本中的对应行：商品按销售稳定行，费用按费用行身份（历史费用按出现顺序）。 */
export function purchaseChangeBaseLine(
    draft: PurchaseChangeDraft,
    line: PurchaseChangeDraftLine,
) {
    const base = draft.base_lines ?? []
    if (line.line_type === "ITEM_SERVICE")
        return base.find(
            (item) =>
                item.line_type === "ITEM_SERVICE" &&
                item.sales_order_line_id === line.sales_order_line_id,
        )
    const expenseId = line.expense?.expense_line_id
    if (expenseId)
        return base.find(
            (item) =>
                item.line_type === "LOGISTICS_FEE" &&
                item.expense?.expense_line_id === expenseId,
        )
    const fees = draft.lines.filter(
        (item) => item.line_type === "LOGISTICS_FEE",
    )
    return base.filter((item) => item.line_type === "LOGISTICS_FEE")[
        fees.indexOf(line)
    ]
}

function isZero(value: string) {
    try {
        return compareDecimal(value.trim() || "0", "0", 6) === 0
    } catch {
        return false
    }
}

/** 商品行数量为零即已移除，原行身份继续保留。 */
export function purchaseChangeLineRemoved(value: PurchaseChangeLineValues) {
    return value.source.line_type === "ITEM_SERVICE" && isZero(value.quantity)
}

/** 本行含税小计；输入不完整时抛错。 */
export function purchaseChangeLineAmount(value: PurchaseChangeLineValues) {
    if (value.source.line_type === "LOGISTICS_FEE")
        return sumFixed([value.grossAmount.trim()], {
            maxScale: 2,
            outputScale: 2,
        })
    return multiplyFixed(value.quantity.trim(), value.unitCostGross.trim(), {
        leftMaxScale: 6,
        rightMaxScale: 6,
        outputScale: 2,
    })
}

/** 改后含税总额；输入不完整时返回 null。 */
export function purchaseChangeTotal(
    lines: readonly PurchaseChangeLineValues[],
) {
    try {
        return sumFixed(lines.map(purchaseChangeLineAmount), {
            maxScale: 2,
            outputScale: 2,
        })
    } catch {
        return null
    }
}

export type PurchaseChangeLineDiff = {
    status: "removed" | "changed" | "unchanged"
    /** 与当前生效版本不同的字段，值为生效版本原值。 */
    previous: Partial<
        Record<
            "quantity" | "unitCostGross" | "inputTaxRate" | "grossAmount",
            string
        >
    >
}

/** 对照当前生效版本，标出本行移除或改动了哪些字段。 */
export function purchaseChangeLineDiff(
    value: PurchaseChangeLineValues,
    base: PurchaseChangeDraftLine | undefined,
): PurchaseChangeLineDiff {
    if (purchaseChangeLineRemoved(value))
        return { status: "removed", previous: {} }
    if (!base) return { status: "unchanged", previous: {} }
    const differs = (next: string, previous: string, scale: number) => {
        try {
            return compareDecimal(next.trim() || "0", previous, scale) !== 0
        } catch {
            return true
        }
    }
    const pairs: [
        keyof PurchaseChangeLineDiff["previous"],
        string,
        string | null,
        number,
    ][] =
        value.source.line_type === "LOGISTICS_FEE"
            ? [["grossAmount", value.grossAmount, base.gross_amount, 2]]
            : [
                  ["quantity", value.quantity, base.quantity, 6],
                  [
                      "unitCostGross",
                      value.unitCostGross,
                      base.unit_cost_gross,
                      6,
                  ],
              ]
    pairs.push(["inputTaxRate", value.inputTaxRate, base.input_tax_rate, 6])
    const previous = Object.fromEntries(
        pairs
            .filter(
                ([, next, old, scale]) =>
                    old !== null && differs(next, old, scale),
            )
            .map(([key, , old]) => [key, compactFixed(old ?? "0")]),
    )
    return {
        status: Object.keys(previous).length ? "changed" : "unchanged",
        previous,
    }
}

/** 保留完整冻结字段，只替换允许修改的数量、价格、税率与交付安排。 */
export function purchaseChangeTargetLines(
    values: PurchaseChangeDraftValues,
): PurchaseChangeDraftLine[] {
    return values.lines.map((value) => {
        const line = value.source
        if (line.line_type === "LOGISTICS_FEE")
            return {
                ...line,
                gross_amount: value.grossAmount.trim(),
                input_tax_rate: value.inputTaxRate.trim(),
            }
        const removed = purchaseChangeLineRemoved(value)
        const quantity = removed ? "0" : value.quantity.trim()
        const rows = removed ? [] : positiveDeliveryRows(value.deliveryRows)
        return {
            ...line,
            quantity,
            allocated_quantity: quantity,
            unit_cost_gross: value.unitCostGross.trim(),
            input_tax_rate: value.inputTaxRate.trim(),
            expected_delivery_date:
                value.expectedDeliveryDate.trim() ||
                line.expected_delivery_date,
            delivery_allocations: rows.length ? rows : undefined,
        }
    })
}

/** 本行数量超出可填范围时的提示；范围未知或合规时返回 undefined。 */
export function purchaseChangeQuantityIssue(
    quantity: string,
    bound: PurchaseQuantityBound | undefined,
) {
    if (!bound) return undefined
    try {
        if (compareDecimal(quantity.trim() || "0", bound.minQuantity, 6) < 0)
            return `扣除已登记退回后，已履约 ${compactFixed(bound.minQuantity)}，不能少于该数量；多出的货物请办理采购退货`
        if (compareDecimal(quantity.trim() || "0", bound.maxQuantity, 6) > 0)
            return bound.quantityAffected
                ? `销售已调减，本单最多保留 ${compactFixed(bound.maxQuantity)}；供应商不接受调减时请登记维持原采购`
                : `最多可采购 ${compactFixed(bound.maxQuantity)}（销售需要扣除其他采购和库存后的剩余量）`
    } catch {
        return undefined
    }
    return undefined
}

/** 改为指定数量后销售行仍待覆盖的数量；为零或无法计算时返回 undefined。 */
export function purchaseChangeGapAfter(
    quantity: string,
    bound: PurchaseQuantityBound | undefined,
) {
    if (!bound) return undefined
    try {
        const capacity = subtractFixed(
            bound.salesQuantity,
            bound.otherCoveredQuantity,
            { maxScale: 6, outputScale: 6 },
        )
        const gap = subtractFixed(capacity, quantity.trim() || "0", {
            maxScale: 6,
            outputScale: 6,
        })
        return compareDecimal(gap, "0", 6) > 0 ? compactFixed(gap) : undefined
    } catch {
        return undefined
    }
}
