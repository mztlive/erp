import {
    compareDecimal,
    minFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { PaymentInput, PaymentRequest } from "../types"

const money = { maxScale: 2, outputScale: 2 } as const

export function positivePaymentAmount(value: string) {
    try {
        return compareDecimal(value, "0", 2) > 0
    } catch {
        return false
    }
}

/** 只使用当前余额；缺少当前依据时不得以提交快照代替。 */
export function paymentBlockReason(
    request: PaymentRequest,
): string | undefined {
    if (request.lines.some((line) => line.source_changed)) {
        return "付款依据已变化，请联系申请人或财务处理原申请。"
    }
    if (
        !request.allowed_actions.includes("PAY") ||
        !["approved", "partially_paid"].includes(request.status)
    ) {
        return "当前申请不可付款，请刷新任务后核对。"
    }
    try {
        if (
            !positivePaymentAmount(request.unpaid_amount) ||
            compareDecimal(
                sumFixed(
                    request.lines.map((line) => line.unpaid_amount),
                    money,
                ),
                request.unpaid_amount,
                2,
            ) !== 0
        ) {
            return "申请待付金额已变化，请刷新后核对。"
        }
        for (const line of request.lines) {
            if (compareDecimal(line.unpaid_amount, "0", 2) < 0) {
                return "申请待付金额异常，请联系财务核对。"
            }
            if (!positivePaymentAmount(line.unpaid_amount)) continue
            if (
                line.current_open_amount == null ||
                line.source_changed == null ||
                !line.payable_entries.length
            ) {
                return "当前付款依据不完整，请刷新后重试。"
            }
            const available = sumFixed(
                line.payable_entries
                    .map((entry) => entry.open_total)
                    .filter(positivePaymentAmount),
                money,
            )
            if (
                compareDecimal(
                    line.current_open_amount,
                    line.unpaid_amount,
                    2,
                ) < 0 ||
                compareDecimal(available, line.unpaid_amount, 2) < 0
            ) {
                return "当前可付款余额已变化，请联系申请人或财务处理原申请。"
            }
        }
    } catch {
        return "当前付款金额无法核对，请刷新后重试。"
    }
}

/** 按冻结申请行顺序消耗未付批准额度，不跨行挪用额度、不跳过失效来源。 */
export function allocateRequestPayment(
    request: PaymentRequest,
    amount: string,
): Pick<PaymentInput, "lines" | "allocations"> {
    const blocked = paymentBlockReason(request)
    if (blocked) throw new Error(blocked)
    if (!positivePaymentAmount(amount))
        throw new Error("请填写大于零且最多两位小数的金额")
    if (compareDecimal(amount, request.unpaid_amount, 2) > 0)
        throw new Error("不能超过本申请剩余待付金额")
    const lines: PaymentInput["lines"] = []
    const allocations: PaymentInput["allocations"] = []
    let remaining = amount
    for (const line of request.lines) {
        if (!positivePaymentAmount(remaining)) break
        const allocated = minFixed(remaining, line.unpaid_amount, money)
        if (!positivePaymentAmount(allocated)) continue
        lines.push({
            payable_account_id: line.payable_account_id,
            amount: allocated,
        })
        let entryRemaining = allocated
        for (const entry of line.payable_entries) {
            if (!positivePaymentAmount(entryRemaining)) break
            if (!positivePaymentAmount(entry.open_total)) continue
            const value = minFixed(entryRemaining, entry.open_total, money)
            allocations.push({
                payable_entry_id: entry.id,
                allocated_amount: value,
            })
            entryRemaining = subtractFixed(entryRemaining, value, money)
        }
        if (positivePaymentAmount(entryRemaining))
            throw new Error("当前可付款余额不足，请刷新后核对。")
        remaining = subtractFixed(remaining, allocated, money)
    }
    if (positivePaymentAmount(remaining))
        throw new Error("付款金额无法分配，请刷新后核对。")
    return { lines, allocations }
}
