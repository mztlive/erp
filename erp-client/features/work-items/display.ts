import { nextActionHintForWorkItemType } from "@/lib/ui-text"
import { displayName } from "@/lib/display-name"

const KNOWN_REASON_LABELS: Record<string, string> = {
    procurement_confirmation_dispatched: "销售已提交，需要采购确认能否供货",
    low_margin_approved_procurement_confirmation:
        "低毛利已获上级通过，需要采购重新确认供货",
    procurement_confirmation_resubmitted:
        "销售已按驳回意见重提，需要采购重新确认",
    procurement_rejection_low_margin_requested:
        "采购驳回后，需要上级确认是否按原条件承接",
    supplier_settlement_review_dispatched: "供应商结算单待复核",
    import_trial_confirmation: "导入试算已完成，需要业务确认范围",
    payable_payment_required: "采购应付已确认，需要安排付款",
    payable_reopened_by_reversal: "付款已冲正，应付余额需要重新安排付款",
    payable_reopened_by_purchase_change:
        "采购变更增加了应付金额，需要继续安排付款",
}

const LEGACY_OWNER_PLACEHOLDERS = new Set(["当前处理人", "处理人待确认"])

export function isUserFacingCopy(value?: string | null): boolean {
    return Boolean(
        value && Array.from(value).some((ch) => ch.charCodeAt(0) > 127),
    )
}

export function normalizeReasonCode(value?: string | null): string {
    return (value ?? "").trim().replace(/-/g, "_").toLowerCase()
}

export function displayReasonLabel(input: {
    reasonLabel?: string | null
    reasonCode?: string | null
}): string {
    const mapped = KNOWN_REASON_LABELS[normalizeReasonCode(input.reasonCode)]
    if (mapped) return mapped
    if (
        isUserFacingCopy(input.reasonLabel) &&
        input.reasonLabel &&
        input.reasonLabel !== "待处理"
    ) {
        return input.reasonLabel
    }
    return "需要你处理"
}

export function displayImpactSummary(input: {
    impactSummary?: string | null
    workItemType?: string | null
    workItemTypeLabel?: string | null
}): string {
    const text = input.impactSummary?.trim() ?? ""
    if (
        isUserFacingCopy(text) &&
        !text.includes("打开业务对象") &&
        !text.startsWith("采购二次确认：") &&
        !text.startsWith("采购单财务审核：")
    ) {
        return text
    }
    if (input.workItemTypeLabel === "采购二次确认") {
        return "不确认则销售单不能生效"
    }
    return "不处理将卡住后续业务，请进入对应页面核对。"
}

export function displayOwnerName(
    value?: string | null,
    ownerId?: string | null,
): string {
    const name = displayName(value, ownerId) ?? ""
    if (!name || LEGACY_OWNER_PLACEHOLDERS.has(name)) return "处理人待确认"
    return name
}

export function displayNextActionHint(input: {
    nextActionHint?: string | null
    workItemType?: string | null
    workItemTypeLabel?: string | null
}): string {
    if (isUserFacingCopy(input.nextActionHint) && input.nextActionHint) {
        return input.nextActionHint
    }
    return nextActionHintForWorkItemType(
        input.workItemType === "PROCUREMENT_ORDER_CREATION"
            ? "待供给分配"
            : input.workItemTypeLabel,
    )
}
