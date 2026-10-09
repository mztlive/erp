import { compareDecimal, subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import { sourcingFormValidationError } from "../purchase-order-create-validation"
import { FULFILLMENT_RESPONSIBILITY_LABEL } from "../../types"
import { findSourcingOption } from "./options"
import { formatQuantityUnits, quantityUnits } from "./quantity"
import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
} from "./types"

export type SourcingEditorRow = {
    product: SourcingProductLine
    allocations: { line: SourcingLineInput; index: number }[]
    issues: string[]
    quantity: string
    remainingQuantity: string
    route: string
    status: string
    needsAttention: boolean
    selected: boolean
    partiallySelected: boolean
}

/** 列表和编辑区共用表单校验与定点数量，不把待确认方案标为已提交。 */
export function buildSourcingEditorRows(
    order: SourcingSalesOrder,
    lines: SourcingLineInput[],
): SourcingEditorRow[] {
    const errors =
        sourcingFormValidationError(order, {
            salesOrderId: order.salesOrderId,
            lines,
        })?.fields ?? {}
    return order.lines.map((product) => {
        const allocations = lines.flatMap((line, index) =>
            line.salesOrderLineId === product.salesOrderLineId
                ? [{ line, index }]
                : [],
        )
        const selected = allocations.filter(({ line }) => line.selected)
        const issues = [
            ...new Set(
                allocations.flatMap(({ index }) =>
                    Object.entries(errors)
                        .filter(([path]) => path.startsWith(`lines[${index}].`))
                        .map(([, error]) => error),
                ),
            ),
        ]
        let quantity = "0"
        let remainingQuantity = "—"
        let partial = false
        try {
            quantity = sumFixed(
                selected.map(({ line }) => line.quantity),
                { maxScale: 6, outputScale: 6 },
            )
            quantity = formatQuantityUnits(quantityUnits(quantity))
            partial = compareDecimal(quantity, product.remainingQuantity, 6) < 0
            remainingQuantity = partial
                ? formatQuantityUnits(
                      quantityUnits(
                          subtractFixed(product.remainingQuantity, quantity, {
                              maxScale: 6,
                              outputScale: 6,
                          }),
                      ),
                  )
                : "0"
        } catch {
            quantity = "—"
        }
        const routes = [
            ...new Set(
                selected.flatMap(({ line }) => {
                    const option = findSourcingOption(product, line.basisId)
                    return option
                        ? [
                              option.sourceType === "EXISTING_STOCK"
                                  ? "现有库存"
                                  : FULFILLMENT_RESPONSIBILITY_LABEL[
                                        option.fulfillmentResponsibility
                                    ],
                          ]
                        : []
                }),
            ),
        ]
        const status = !selected.length
            ? "暂不分配"
            : issues.some((issue) => issue.includes("请选择采购入库目标仓"))
              ? "缺目标仓"
              : issues.length
                ? "待调整"
                : partial
                  ? "部分分配"
                  : "已就绪"
        return {
            product,
            allocations,
            issues,
            quantity,
            remainingQuantity,
            route: routes.join(" / ") || "未选来源",
            status,
            needsAttention:
                selected.length > 0 && (issues.length > 0 || partial),
            selected:
                allocations.length > 0 &&
                selected.length === allocations.length,
            partiallySelected:
                selected.length > 0 && selected.length < allocations.length,
        }
    })
}
