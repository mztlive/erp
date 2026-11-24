import {
    clampZeroFixed,
    compactFixed,
    compareDecimal,
    divideFixed,
    minFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { FulfillmentDraft, FulfillmentOperation } from "../../types"

const quantityPrecision = { maxScale: 6, outputScale: 6 } as const

export function positiveQuantity(value: string) {
    try {
        return compareDecimal(value, "0", quantityPrecision.maxScale) > 0
    } catch {
        return false
    }
}

function quantityTotal(values: readonly string[]) {
    return compactFixed(
        sumFixed(
            values.map((value) => (value.trim() ? value : "0")),
            quantityPrecision,
        ),
    )
}

function groupQuantity(
    lines: readonly { unitCode: string; unitName?: string; quantity: string }[],
) {
    const groups = new Map<string, { label: string; values: string[] }>()
    for (const { unitCode, unitName, quantity } of lines) {
        const key = unitCode || unitName || ""
        const group = groups.get(key) ?? { label: unitName || "", values: [] }
        if (!group.label && unitName) group.label = unitName
        group.values.push(quantity)
        groups.set(key, group)
    }
    if (groups.size > 1 && [...groups.values()].some((group) => !group.label))
        return "—"
    return (
        [...groups.values()]
            .map(({ label, values }) =>
                [quantityTotal(values), label].filter(Boolean).join(" "),
            )
            .join("、") || "—"
    )
}

export function fulfillmentBatchQuantities(
    operation: FulfillmentOperation,
    draft: FulfillmentDraft,
) {
    const quantities = new Map<string, string[]>()
    const add = (id: string, quantity: string) => {
        const values = quantities.get(id) ?? []
        values.push(quantity)
        quantities.set(id, values)
    }
    if (draft.type === "RECEIPT") {
        draft.lines.forEach((line) =>
            add(line.purchaseRevisionLineId, line.receivedQuantity),
        )
    } else if (
        (draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT") &&
        draft.packing
    ) {
        draft.packing.dispatch.forEach((row) =>
            row.items.forEach((item) =>
                add(item.delivery_line_id, item.quantity),
            ),
        )
    } else {
        draft.lines.forEach((line) => add(line.salesOrderLineId, line.quantity))
    }

    const lines = operation.lines.map((line) => ({
        ...line,
        quantities:
            quantities.get(
                draft.type === "RECEIPT"
                    ? (line.purchaseRevisionLineId ?? "")
                    : (draft.type === "WAREHOUSE_SHIP" ||
                            draft.type === "SUPPLIER_DIRECT") &&
                        draft.packing
                      ? line.lineId
                      : line.salesOrderLineId,
            ) ?? [],
    }))
    const pending = readQuantity(() =>
        groupQuantity(
            lines.map((line) => ({
                ...line,
                quantity: line.remainingQuantity,
            })),
        ),
    )
    const progress = readQuantity(() => {
        // 先算每行的完成比例再取平均，避免跨单位相加。
        const ratios = lines
            .filter((line) => positiveQuantity(line.orderedQuantity))
            .map((line) => {
                const completed = clampZeroFixed(
                    subtractFixed(
                        line.orderedQuantity,
                        line.remainingQuantity,
                        quantityPrecision,
                    ),
                    quantityPrecision,
                )
                return divideFixed(
                    minFixed(
                        completed,
                        line.orderedQuantity,
                        quantityPrecision,
                    ),
                    line.orderedQuantity,
                    {
                        numeratorMaxScale: 6,
                        denominatorMaxScale: 6,
                        outputScale: 6,
                    },
                )
            })
        if (!ratios.length) return "—"
        return compactFixed(
            divideFixed(
                multiplyFixed(quantityTotal(ratios), "100", {
                    leftMaxScale: 6,
                    rightMaxScale: 0,
                    outputScale: 6,
                }),
                String(ratios.length),
                {
                    numeratorMaxScale: 6,
                    denominatorMaxScale: 0,
                    outputScale: 1,
                },
            ),
        )
    })
    const lineCount = lines.filter((line) =>
        line.quantities.some(positiveQuantity),
    ).length
    const total = readQuantity(() =>
        groupQuantity(
            lines.map((line) => ({
                ...line,
                quantity: quantityTotal(line.quantities),
            })),
        ),
    )
    const remaining = readQuantity(() =>
        groupQuantity(
            lines.map((line) => ({
                ...line,
                quantity: clampZeroFixed(
                    subtractFixed(
                        line.remainingQuantity,
                        quantityTotal(line.quantities),
                        quantityPrecision,
                    ),
                    quantityPrecision,
                ),
            })),
        ),
    )
    return { pending, progress, lineCount, total, remaining }
}

function readQuantity(read: () => string) {
    try {
        return read()
    } catch {
        return "—"
    }
}
