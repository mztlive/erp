import { compareDecimal, subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import type { PaymentRequest, RequestLine } from "../types"

const money = { maxScale: 4, outputScale: 4, allowNegative: true } as const

/** 当前占用包含本申请；审核额度须加回本申请未付部分，历史版本只用冻结依据。 */
export function paymentReviewBasis(
    request: PaymentRequest,
    historical = false,
) {
    const current =
        !historical &&
        request.lines.length > 0 &&
        request.lines.every(
            (line) =>
                line.current_total_amount != null &&
                line.current_settled_amount != null &&
                line.current_open_amount != null &&
                line.current_reserved_amount != null,
        )
    const occupies = ["in_approval", "approved", "partially_paid"].includes(
        request.status,
    )
    const lines = request.lines.map((line) => {
        const reserved = current
            ? subtractFixed(
                  line.current_reserved_amount!,
                  occupies ? line.unpaid_amount : "0",
                  money,
              )
            : line.reserved_amount
        return {
            total: current ? line.current_total_amount! : line.total_amount,
            paid: current
                ? line.current_settled_amount!
                : line.source_paid_amount,
            reserved,
            available: subtractFixed(
                current ? line.current_open_amount! : line.open_amount,
                reserved,
                money,
            ),
        }
    })
    return {
        current,
        total: sumFixed(
            lines.map((line) => line.total),
            money,
        ),
        paid: sumFixed(
            lines.map((line) => line.paid),
            money,
        ),
        reserved: sumFixed(
            lines.map((line) => line.reserved),
            money,
        ),
        available: sumFixed(
            lines.map((line) => line.available),
            money,
        ),
        exceeds: lines.some(
            (line, index) =>
                compareDecimal(
                    current
                        ? request.lines[index].unpaid_amount
                        : request.lines[index].requested_amount,
                    line.available,
                    4,
                ) > 0,
        ),
    }
}

export function purchaseSummary(lines: RequestLine[]) {
    const names = [
        ...new Set(
            lines.flatMap((line) =>
                line.purchase_items.map((item) => item.item_name),
            ),
        ),
    ]
    return names.length
        ? `${names.slice(0, 2).join("、")}${names.length > 2 ? "等" : ""}采购款`
        : "采购付款"
}

export const fulfillmentLabel: Record<string, string> = {
    NONE: "尚未履约",
    PARTIAL: "部分履约",
    COMPLETED: "已完成履约",
    PENDING: "待履约",
    NOT_STARTED: "尚未履约",
    COMPLETE: "已完成履约",
}
