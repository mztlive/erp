import { z } from "zod"
import {
    compareDecimal,
    formatCurrencyFixed,
    minFixed,
    normalizeFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import {
    isSupplierPaymentTermCode,
    paymentTermCode,
} from "@/lib/business-options"
import type { PaymentCandidate, PaymentRequest } from "../types"

export const groupKey = (candidate: PaymentCandidate) =>
    `${candidate.supplier_id}:${candidate.our_company_id}`
export function positive(value: string) {
    try {
        return compareDecimal(value, "0", 2) > 0
    } catch {
        return false
    }
}
/** 仅使用生效采购条款；已付由预付款缺口扣除，在途申请另行扣除。 */
export function suggestedRequestAmount(candidate: PaymentCandidate): string {
    const code = paymentTermCode(candidate.payment_term_code ?? "")
    if (
        !code ||
        !isSupplierPaymentTermCode(code) ||
        !positive(candidate.available_amount)
    )
        return ""
    const prepay = code.startsWith("PREPAY_")
    if (candidate.prepay_gate !== prepay) return ""
    const options = { maxScale: 2, outputScale: 2 } as const
    if (!prepay) return normalizeFixed(candidate.available_amount, options)
    if (!candidate.prepayment_gap || !positive(candidate.prepayment_gap))
        return ""
    const remaining = subtractFixed(
        candidate.prepayment_gap,
        candidate.reserved_amount,
        options,
    )
    return positive(remaining)
        ? minFixed(remaining, candidate.available_amount, options)
        : ""
}

/** 首次选中时预填；用户修改或主动清空后，返回选单不得覆盖。 */
export function prefillRequestAmounts(
    ids: string[],
    candidates: PaymentCandidate[],
    amounts: Record<string, string>,
) {
    const next = { ...amounts }
    for (const candidate of candidates) {
        const id = candidate.payable_account_id
        if (ids.includes(id) && next[id] === undefined)
            next[id] = suggestedRequestAmount(candidate)
    }
    return next
}

export function requestAmountError(
    value: string,
    available: string,
): string | undefined {
    if (!positive(value)) return "请填写大于零且最多两位小数的金额"
    if (compareDecimal(value, available, 2) > 0)
        return `本次最多可申请 ${formatCurrencyFixed(available, { maxScale: 2, minimumFractionDigits: 2 })}`
    return undefined
}
export function totalOf(values: string[]) {
    return sumFixed(values.filter(positive), { maxScale: 2, outputScale: 2 })
}
export type DraftGroup = {
    key: string
    ids: string[]
    account: string
    date: string
    description: string
    files: File[]
}
export type DraftValues = {
    selectedIds: string[]
    amounts: Record<string, string>
    groups: DraftGroup[]
}
/** 希望付款日默认业务当天，不表示合同到期日或实际付款日。 */
function defaultRequestDate(): string {
    const parts = new Intl.DateTimeFormat("en", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
    }).formatToParts(new Date())
    const part = (type: string) =>
        parts.find((item) => item.type === type)!.value
    return `${part("year")}-${part("month")}-${part("day")}`
}

export function regroup(
    ids: string[],
    candidates: PaymentCandidate[],
    previous: DraftGroup[],
): DraftGroup[] {
    const selected = new Set(ids)
    const groups = previous.map((group) => ({
        ...group,
        ids: group.ids.filter((id) => selected.has(id)),
    }))
    const assigned = new Set(groups.flatMap((group) => group.ids))
    for (const candidate of candidates) {
        const id = candidate.payable_account_id
        if (!selected.has(id) || assigned.has(id)) continue
        const key = groupKey(candidate)
        const existing = groups.find((group) => group.key === key)
        if (existing) existing.ids.push(id)
        else
            groups.push({
                key,
                ids: [id],
                account: candidate.payment_recipient?.bank_account_id ?? "",
                date: defaultRequestDate(),
                description: "",
                files: [],
            })
    }
    return groups.filter((group) => group.ids.length)
}
export function draftDefaults(
    candidates: PaymentCandidate[],
    purchaseId?: string,
    request?: PaymentRequest,
): DraftValues {
    const ids = request
        ? request.lines.map((line) => line.payable_account_id)
        : candidates
              .filter((item) => item.purchase_order_id === purchaseId)
              .map((item) => item.payable_account_id)
    const groups = regroup(ids, candidates, [])
    if (request && groups[0])
        Object.assign(groups[0], {
            account: request.payment_recipient.bank_account_id,
            date: request.desired_payment_date,
            description: request.description,
        })
    return {
        selectedIds: ids,
        groups,
        amounts: request
            ? Object.fromEntries(
                  request.lines.map((line) => [
                      line.payable_account_id,
                      line.requested_amount,
                  ]),
              )
            : prefillRequestAmounts(ids, candidates, {}),
    }
}
export function draftSchema(candidates: PaymentCandidate[]) {
    const lookup = new Map(
        candidates.map((candidate) => [
            candidate.payable_account_id,
            candidate,
        ]),
    )
    return z
        .object({
            selectedIds: z.array(z.string()).min(1, "请至少选择一张采购单"),
            amounts: z.record(z.string(), z.string()),
            groups: z
                .array(
                    z.object({
                        key: z.string(),
                        ids: z.array(z.string()).min(1),
                        account: z.string().min(1, "请选择有效收款账户"),
                        date: z.string().min(1, "请填写希望付款日"),
                        description: z
                            .string()
                            .trim()
                            .min(1, "请填写申请说明")
                            .max(2000, "申请说明最多 2000 字"),
                        files: z.array(z.custom<File>()),
                    }),
                )
                .min(1)
                .max(50, "每次最多保存 50 份申请"),
        })
        .superRefine((values, context) => {
            const grouped = values.groups.flatMap((group) => group.ids)
            if (
                new Set(grouped).size !== grouped.length ||
                grouped.length !== values.selectedIds.length ||
                values.selectedIds.some((id) => !grouped.includes(id))
            ) {
                context.addIssue({
                    code: "custom",
                    path: ["selectedIds"],
                    message: "部分采购来源已不可用，请返回上一步重新选择",
                })
            }
            for (const [index, group] of values.groups.entries()) {
                const first = lookup.get(group.ids[0])
                if (
                    !first?.bank_accounts.some(
                        (account) => account.bank_account_id === group.account,
                    )
                )
                    context.addIssue({
                        code: "custom",
                        path: ["groups", index, "account"],
                        message: "请选择当前供应商的有效账户",
                    })
                for (const id of group.ids) {
                    const candidate = lookup.get(id)
                    const amount = values.amounts[id] ?? ""
                    const message = !candidate
                        ? "采购来源已不可用，请重新选择"
                        : requestAmountError(amount, candidate.available_amount)
                    if (message)
                        context.addIssue({
                            code: "custom",
                            path: ["amounts", id],
                            message,
                        })
                }
            }
        })
}
