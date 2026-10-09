import type { AllocationSessionView } from "@/features/supplier-payables/types"
import { cents, fromCents } from "./allocation-model"

type PaymentTarget = AllocationSessionView["pool"][number]

/** 采购依据不完整或尾款条件不明确时留空，不回退整单余额。 */
export function suggestedPayment(item: PaymentTarget): string {
    return item.paymentGuidance?.suggestedAmount ?? ""
}

export function paymentBasisNote(item: PaymentTarget): string {
    const guidance = item.paymentGuidance
    if (!guidance) return "付款依据未加载，请核对采购单后填写实际付款金额。"
    if (guidance.prepayGate && guidance.requiredPrepayment === null) {
        return "先款条件缺少付款门槛，请核对采购单并补齐条款。"
    }
    if (
        guidance.prepayGate &&
        cents(guidance.prepaymentGap ?? "0") === BigInt(0)
    ) {
        return cents(item.openTotal) > BigInt(0)
            ? "预付款已满足；尾款付款条件未明确，请核对合同后登记实际付款。"
            : "预付款已满足，整单已结清。"
    }
    if (guidance.suggestedAmount === null)
        return "付款条件待核对，请按实际付款登记。"
    return guidance.prepayGate
        ? "建议金额为尚未满足的预付款；实际付款须与银行回单一致。"
        : "本次建议按剩余应付填写；付款时间以采购约定为准。"
}

/** 金额输入中的预计结果，提交仍由后端重验余额、责任与版本。 */
export function paymentOutcome(item: PaymentTarget, amount: string): string {
    const paid = cents(amount)
    if (paid <= BigInt(0)) return "请填写本次实际付款金额。"
    const remaining = cents(item.openTotal) - paid
    if (remaining < BigInt(0)) return "本次金额超过整单剩余应付，请核对。"
    const guidance = item.paymentGuidance
    const balance =
        remaining === BigInt(0)
            ? "整单结清"
            : `剩余应付 ¥${fromCents(remaining)}`
    if (!guidance) return `登记后${balance}。`
    const cumulative = cents(guidance.paidTotal) + paid
    let result = `登记后累计已付 ¥${fromCents(cumulative)} / ¥${guidance.purchaseTotal}，${balance}。`
    if (guidance.prepayGate && guidance.requiredPrepayment !== null) {
        const gap = cents(guidance.requiredPrepayment) - cumulative
        result +=
            gap > BigInt(0)
                ? `预付款还差 ¥${fromCents(gap)}。`
                : "预付款条件已满足。"
        const previousGap = cents(guidance.prepaymentGap ?? "0")
        if (paid > previousGap)
            result += `本次比预付款缺口多付 ¥${fromCents(paid - previousGap)}。`
    }
    return result
}
