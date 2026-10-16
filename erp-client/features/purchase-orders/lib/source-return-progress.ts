import type { PurchaseOrderCenterView } from "../types"
import { compareDecimal, subtractFixed } from "@/lib/fixed-decimal"
export function sourceReturnProgress(order: PurchaseOrderCenterView) {
    const plan = order.sourceUpdate?.returnHandling
    const physicalPending = !!plan?.lines.some(
        (line) => compareDecimal(line.required_return, "0", 6) > 0,
    )
    const hasReturns = !!plan?.return_numbers.length
    const refund =
        plan && order.payableSummary
            ? subtractFixed(
                  order.payableSummary.paidAllocatedAmount,
                  plan.target_gross,
                  { maxScale: 2, outputScale: 2 },
              )
            : "0"
    const redInvoice =
        plan && order.payableSummary
            ? subtractFixed(
                  order.payableSummary.purchaseInvoiceAllocatedAmount,
                  plan.target_gross,
                  { maxScale: 2, outputScale: 2 },
              )
            : "0"
    const refundPending = compareDecimal(refund, "0", 6) > 0
    const invoicePending = compareDecimal(redInvoice, "0", 6) > 0
    return {
        plan,
        physicalPending,
        hasReturns,
        hasOpenReturns: plan?.pending_settlement ?? false,
        refund,
        redInvoice,
        refundPending,
        invoicePending,
        blocksChange: physicalPending || refundPending || invoicePending,
    }
}
