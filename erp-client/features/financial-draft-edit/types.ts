import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"

export type FinancialDraftKind =
    | "customer_receipt"
    | "customer_refund"
    | "receipt_reversal"
    | "supplier_refund"
    | "payment_reversal"

export type FinancialDraftSide = "customer" | "supplier"

/** 草稿编辑只消费可修改字段、身份、版本与服务端审批动作。 */
export type FinancialDraft = {
    id: string
    version: number
    status: string
    amount: string
    receipt_no?: string
    refund_no?: string
    reversal_no?: string
    received_at?: number
    bank_reference?: string | null
    reason_text?: string
    handled_by?: string
    reviewed_by?: string
    pending_allocations?: FinancialDraftAllocation[]
    approval?: DocumentApprovalViewDto | null
}

export type FinancialDraftAllocation = {
    receivable_entry_id: string
    allocated_amount: string
}

export type FinancialDraftFormValues = {
    amount: string
    receivedAt: string
    bankReference: string
    reasonText: string
    allocations: { receivableEntryId: string; allocatedAmount: string }[]
}

/** 财务实体草稿状态采用接口的 snake_case 序列化值。 */
export function isFinancialDraft(draft: FinancialDraft) {
    return draft.status === "draft"
}

/** 回款专用读取已校验原创建人，逆向原单要求原经办且经办复核分离。 */
export function isFinancialDraftEditor(
    kind: FinancialDraftKind,
    draft: FinancialDraft,
    userId?: string,
) {
    return (
        kind === "customer_receipt" ||
        Boolean(
            userId &&
            draft.handled_by === userId &&
            draft.reviewed_by !== userId,
        )
    )
}

export const FINANCIAL_DRAFTS: Record<
    FinancialDraftKind,
    {
        resource: string
        label: string
        side: FinancialDraftSide
        preview: string
    }
> = {
    customer_receipt: {
        resource: "customer-receipts",
        label: "客户回款单",
        side: "customer",
        preview: "receipt",
    },
    customer_refund: {
        resource: "customer-refunds",
        label: "客户退款单",
        side: "customer",
        preview: "refund",
    },
    receipt_reversal: {
        resource: "receipt-reversals",
        label: "回款冲正单",
        side: "customer",
        preview: "reversal",
    },
    supplier_refund: {
        resource: "supplier-refunds",
        label: "供应商退款单",
        side: "supplier",
        preview: "refund",
    },
    payment_reversal: {
        resource: "payment-reversals",
        label: "付款冲正单",
        side: "supplier",
        preview: "reversal",
    },
}

/** 深链对象类型只接受已支持的财务草稿。 */
export function parseFinancialDraftKind(
    raw: string | null,
    side: FinancialDraftSide,
): FinancialDraftKind | undefined {
    if (!raw || !Object.hasOwn(FINANCIAL_DRAFTS, raw)) return undefined
    const kind = raw as FinancialDraftKind
    return FINANCIAL_DRAFTS[kind].side === side ? kind : undefined
}

/** 原单编辑深链；打开时只显示编辑窗口，关闭后再进入详情。 */
export function financialDraftEditHref(kind: FinancialDraftKind, id: string) {
    const definition = FINANCIAL_DRAFTS[kind]
    const params = new URLSearchParams({
        view: definition.side === "customer" ? "receipt" : "payment",
        editType: kind,
        editId: id,
    })
    return `/finance/${definition.side}-accounts?${params}`
}

/** 回款时间按界面固定的上海时区回填，保留秒。 */
export function receivedAtFormValue(seconds?: number): string {
    if (seconds == null) return ""
    return new Date((seconds + 8 * 60 * 60) * 1000).toISOString().slice(0, 19)
}

/** 回款日期时间选择器使用上海时区，转换为接口 Unix 秒。 */
export function receivedAtUnixSeconds(value: string): number | null {
    if (!value) return null
    const milliseconds = new Date(`${value}+08:00`).getTime()
    return Number.isFinite(milliseconds)
        ? Math.floor(milliseconds / 1000)
        : null
}

export function financialDraftFormValues(
    draft: FinancialDraft,
): FinancialDraftFormValues {
    return {
        amount: draft.amount,
        receivedAt: receivedAtFormValue(draft.received_at),
        bankReference: draft.bank_reference ?? "",
        reasonText: draft.reason_text ?? "",
        allocations: (draft.pending_allocations ?? []).map((line) => ({
            receivableEntryId: line.receivable_entry_id,
            allocatedAmount: line.allocated_amount,
        })),
    }
}
