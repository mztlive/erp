/** W12 供应商往来 · 核销提交前校验问题构建（纯函数，无 React）。 */

import type { ValidationIssue } from "@/components/business"
import { cents } from "@/features/supplier-payables/lib/allocation-model"
import type {
    AllocationSessionView,
    AllocationTrack,
} from "@/features/supplier-payables/types"

export type AllocationIssueInput = {
    track: AllocationTrack
    selected: ReadonlySet<string>
    amounts: Record<string, string>
    pool: AllocationSessionView["pool"] | undefined
    /** 拟分配合计（已按元格式化） */
    allocatedHint: string
    factAmount: string
    existingInvoiceId?: string
    existingUnallocated?: string
    existingAmount?: string
}

export function buildAllocationIssues(
    input: AllocationIssueInput,
): ValidationIssue[] {
    const {
        track,
        selected,
        amounts,
        pool,
        allocatedHint,
        factAmount,
        existingInvoiceId,
        existingUnallocated,
        existingAmount,
    } = input

    const issues: ValidationIssue[] = []
    if (selected.size === 0) {
        issues.push({
            id: "no-target",
            label: "核销目标",
            message: "请至少选择一笔同供应商应付",
            targetId: "alloc-pool",
        })
    }
    const capAmount = existingUnallocated || existingAmount || factAmount || "0"
    if (cents(factAmount || "0") <= BigInt(0) && !existingInvoiceId) {
        issues.push({
            id: "amount",
            label: track === "payment" ? "付款金额" : "发票金额",
            message: "金额必须为正数",
        })
    }
    if (cents(allocatedHint) > cents(capAmount)) {
        issues.push({
            id: "over",
            label: "拟分配",
            message: "拟分配合计超过本次记录金额，最终以系统校验为准",
        })
    }
    for (const id of selected) {
        const item = pool?.find((p) => p.payableAccountId === id)
        if (!item) continue
        const open =
            track === "payment" ? item.openTotal : item.openInvoiceableTotal
        if (cents(amounts[id] ?? "0") > cents(open)) {
            issues.push({
                id: `over-${id}`,
                label: item.sourceDocumentNo,
                message: `拟分配超过开放余额 ${open}`,
            })
        }
        if (cents(amounts[id] ?? "0") <= BigInt(0)) {
            issues.push({
                id: `zero-${id}`,
                label: item.sourceDocumentNo,
                message: "分配金额须为正数",
            })
        }
    }
    return issues
}
