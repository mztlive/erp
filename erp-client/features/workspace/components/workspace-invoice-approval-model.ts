import type { WorkspaceApprovalResubmissionBannerProps } from "@/components/business"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { approvalSalesTaxPoint } from "@/features/approval-workflow/display"
import { resolveInvoiceRequestLines } from "@/features/invoice-requests/components/request-details"
import {
    invoiceLinesTotal,
    isInvoiceAmountText,
} from "@/features/invoice-requests/line-amounts"
import { displayBusinessText, displayName } from "@/lib/display-name"
import {
    clampZeroFixed,
    compareDecimal,
    divideFixed,
    minFixed,
    multiplyFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { InvoiceApprovalReview } from "../api/invoice-review"
import type { WorkspaceWorkItem } from "../types"

export type InvoiceSource = NonNullable<
    ApprovalMaterials["display"]["source_sales"]
>[number]

const invoiceMoneyScale = { maxScale: 2, outputScale: 2 }

function negateDecimal(value: string): string {
    const trimmed = value.trim()
    return trimmed.startsWith("-") ? trimmed.slice(1) : `-${trimmed}`
}

export function invoiceApprovalRisks(review?: InvoiceApprovalReview) {
    let shortage: string | undefined
    if (review) {
        try {
            if (compareDecimal(review.amounts.shortage, "0", 2) > 0) {
                shortage = review.amounts.shortage
            }
        } catch {
            shortage = undefined
        }
    }
    const warnings = [
        ...new Set(
            review?.warnings.map((warning) => warning.trim()).filter(Boolean) ??
                [],
        ),
    ]
    const isQuotaWarning = (warning: string) =>
        Boolean(shortage && warning.startsWith("当前开票占用超出未开票额度"))

    return {
        shortage,
        quotaWarnings: warnings.filter(isQuotaWarning),
        sourceWarnings: warnings.filter((warning) => !isQuotaWarning(warning)),
    }
}

export function buildInvoiceDynamicRejectPhrases(
    review?: InvoiceApprovalReview,
    attachmentCount?: number,
): string[] {
    const { shortage, sourceWarnings } = invoiceApprovalRisks(review)
    const phrases = [...sourceWarnings]
    if (shortage) {
        phrases.unshift(
            `当前开票占用超额 ¥${shortage}，请核对本次金额及其他申请占用。`,
        )
    }
    if (review?.records.receipt_count === 0) {
        phrases.push(
            "本笔应收暂无关联回款，请补充回款凭证或说明合同约定的开票时点。",
        )
    }
    if (attachmentCount === 0) {
        phrases.push("请补充与本次开票申请相关的合同或凭证。")
    }
    return [...new Set(phrases)].slice(0, 6)
}

export function invoiceApprovalQuota(review: InvoiceApprovalReview) {
    const { invoiceable_total, invoiced_total, amounts } = review
    const reserved = sumFixed(
        [
            amounts.current_reserved,
            amounts.other_pending,
            amounts.other_approved,
        ],
        invoiceMoneyScale,
    )
    const beforeRequest = sumFixed(
        [
            invoiceable_total,
            negateDecimal(invoiced_total),
            negateDecimal(amounts.other_pending),
            negateDecimal(amounts.other_approved),
        ],
        { ...invoiceMoneyScale, allowNegative: true },
    )
    return {
        reserved,
        beforeRequest: clampZeroFixed(beforeRequest, invoiceMoneyScale),
    }
}

export function invoiceProgressPercent(
    amount: string,
    total: string,
): string | undefined {
    try {
        if (compareDecimal(total, "0", 2) <= 0) return undefined
        const ratio = divideFixed(
            clampZeroFixed(amount, invoiceMoneyScale),
            total,
            { numeratorMaxScale: 2, denominatorMaxScale: 2, outputScale: 4 },
        )
        return minFixed(ratio, "1", { maxScale: 4, outputScale: 4 })
    } catch {
        return undefined
    }
}

export function invoiceProgressRate(
    amount: string,
    total: string,
): string | undefined {
    try {
        if (compareDecimal(total, "0", 2) <= 0) return undefined
        const ratio = divideFixed(
            clampZeroFixed(amount, invoiceMoneyScale),
            total,
            { numeratorMaxScale: 2, denominatorMaxScale: 2, outputScale: 4 },
        )
        return multiplyFixed(ratio, "100", {
            leftMaxScale: 4,
            rightMaxScale: 0,
            outputScale: 1,
        })
    } catch {
        return undefined
    }
}

export function invoiceApprovalSubmission(
    materials: ApprovalMaterials | undefined,
    item: WorkspaceWorkItem,
    review: InvoiceApprovalReview | undefined,
) {
    const source = materials?.display.source
    const fields = new Map(
        (item.summarySections ?? []).map(({ label, value }) => [
            label,
            value.trim(),
        ]),
    )
    for (const { label, value } of source?.extra_sections ?? []) {
        if (value.trim()) fields.set(label, value.trim())
    }

    const snapshot = materials?.display.invoice_request
    const data = snapshot ?? {
        invoice_title:
            fields.get("购方名称") ??
            fields.get("开票抬头") ??
            source?.customer ??
            "",
        tax_number: fields.get("购方纳税人识别号") ?? fields.get("税号") ?? "",
        seller_name: fields.get("销方名称"),
        seller_tax_number: fields.get("销方纳税人识别号"),
        invoice_content: fields.get("开票内容") ?? "",
        reason: fields.get("申请事由") ?? "",
    }
    const lines = resolveInvoiceRequestLines(data)
    const lineTotal = invoiceLinesTotal(lines)
    const summaryAmount = item.amountSummary?.value.replace(/[¥￥,\s元]/g, "")
    const amount =
        snapshot?.amount ??
        review?.requests.find((request) => request.is_current)?.amount ??
        fields.get("申请金额") ??
        (isInvoiceAmountText(summaryAmount) ? summaryAmount : lineTotal)
    const submittedSales = materials?.display.source_sales ?? []
    const currentBasis = materials?.current_sales_basis
    const sales = submittedSales[0] ?? currentBasis?.source ?? review?.source
    const salesFields = new Map(
        sales?.source.extra_sections.map(({ label, value }) => [label, value]),
    )
    const salesRevision =
        materials?.source_sales_orders?.find(
            ({ revision }) => revision.id === sales?.revision_id,
        )?.revision ??
        (currentBasis?.source.revision_id === sales?.revision_id
            ? currentBasis?.sales_order.revision
            : undefined)
    const invoiceType =
        fields.get("开票类型") ??
        salesFields.get("开票要求") ??
        salesRevision?.invoice_type

    return {
        amount,
        lineTotal,
        lines,
        buyerTitle: displayName(data.invoice_title),
        buyerTaxNumber: data.tax_number?.trim(),
        sellerName: displayName(data.seller_name),
        sellerTaxNumber: data.seller_tax_number?.trim(),
        reason: data.reason?.trim(),
        submitter:
            displayName(source?.submitter_name) ??
            displayName(fields.get("申请人")) ??
            displayName(fields.get("提交人")),
        invoiceType,
        paymentTerm:
            salesFields.get("付款条件") ?? salesRevision?.payment_term_name,
        taxPoint: (() => {
            const formatted = salesRevision?.tax_point
                ? approvalSalesTaxPoint(salesRevision.tax_point)
                : undefined
            return formatted && formatted !== "—" ? formatted : undefined
        })(),
        invoiceTiming: salesFields.get("开票时点"),
        accountLabel: review?.account_label,
    }
}

export function invoiceSourceNumber(source: InvoiceSource) {
    return (
        displayBusinessText(source.document_no, source.document_id) ||
        "销售单号未记录"
    )
}

export function invoiceApprovalResubmission(
    materials: ApprovalMaterials | undefined,
    item: WorkspaceWorkItem,
): WorkspaceApprovalResubmissionBannerProps | null {
    const roundNo = item.approval?.currentRoundNo
    if (roundNo == null || roundNo < 2) return null

    const fields = new Map(
        (item.summarySections ?? []).map(({ label, value, numeric }) => [
            label,
            { value, numeric },
        ]),
    )
    for (const { label, value, numeric } of materials?.display.source
        .extra_sections ?? []) {
        fields.set(label, { value, numeric })
    }
    const suffix = "（前→后）"
    const diffs = Array.from(fields).flatMap(([label, { value, numeric }]) => {
        if (!label.endsWith(suffix)) return []
        const separator = " → "
        const index = value.indexOf(separator)
        if (index < 0) return []
        const before = value.slice(0, index).trim()
        const after = value.slice(index + separator.length).trim()
        return before && after
            ? [
                  {
                      label: label.slice(0, -suffix.length),
                      before,
                      after,
                      numeric,
                  },
              ]
            : []
    })
    const reason = item.approval?.lastRejectReason?.trim()
    const previousRejection =
        reason && item.approval?.status !== "REJECTED"
            ? { reason, actor: item.approval?.lastRejectorLabel }
            : undefined
    const resubmissionReason = fields.get("重新提交说明")?.value.trim()

    return {
        roundNo,
        diffBaseline: fields.get("对比基准")?.value,
        diffSummary:
            fields.get("重提差异")?.value.trim() ||
            (!resubmissionReason && !previousRejection && !diffs.length
                ? "未提供重提变更说明"
                : undefined),
        diffs,
        resubmissionReason,
        previousRejection,
    }
}
