"use client"

import Link from "next/link"

import { MoneyValue } from "@/components/business"
import {
    PreviewAmount,
    PreviewSection,
    PreviewFact,
    PreviewNote,
} from "@/components/business/financial-preview"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PaymentReversalApprovalArea } from "@/features/supplier-payables/components/payment-reversal-approval-area"
import { useSupplierPaymentQuery } from "@/features/supplier-payables/hooks/queries"
import { paymentPreviewHref } from "@/features/supplier-payables/lib/related-documents"
import { paymentReversalApprovalPhase } from "@/features/supplier-payables/lib/payment-reversal-approval"
import type { PaymentReversalRow } from "@/features/supplier-payables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"

/**
 * 付款冲正详情。草稿展示绑定卡，运行中/终态嵌入通用审批区。
 */
export function PaymentReversalDetailBody({
    row,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    row: PaymentReversalRow
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const posted = row.status === "posted" || row.status === "reversed"
    const originalPaymentQuery = useSupplierPaymentQuery(row.originalPaymentId)
    const originalPayment = originalPaymentQuery.data
    return (
        <div className="min-h-0 flex-1 space-y-6 overflow-auto px-7 py-6 text-sm">
            <PreviewAmount label="冲正金额" value={row.amount} />
            <PreviewSection title="原因说明">
                <p className="leading-6">{row.reasonText || "未填写原因"}</p>
                <PreviewNote>
                    冲正时间：
                    {formatDateTime(row.occurredAt, "full", "passthrough")}
                </PreviewNote>
            </PreviewSection>
            <PreviewSection title="原付款">
                <dl className="space-y-3">
                    <PreviewFact label="付款单号">
                        <span className="num">
                            {originalPaymentQuery.isPending
                                ? "正在读取…"
                                : (originalPayment?.paymentNo ??
                                  "付款信息待补全")}
                        </span>
                    </PreviewFact>
                    <PreviewFact label="供应商">
                        {originalPayment?.supplierName ?? "供应商信息待补全"}
                    </PreviewFact>
                    {originalPayment ? (
                        <PreviewFact label="原付款金额">
                            <MoneyValue value={originalPayment.amount} />
                        </PreviewFact>
                    ) : null}
                </dl>{" "}
                <Button
                    id={`supplier-payables-reversal-detail-${toAutomationIdSegment(row.reversalId)}-open-original`}
                    type="button"
                    size="xs"
                    variant="outline"
                    className="mt-1"
                    render={
                        <Link
                            href={paymentPreviewHref(row.originalPaymentId)}
                        />
                    }
                >
                    打开原付款
                </Button>
            </PreviewSection>
            <PreviewSection title="审批记录">
                <PaymentReversalApprovalArea
                    phase={paymentReversalApprovalPhase(
                        row.approval,
                        row.status === "in_approval"
                            ? "IN_APPROVAL"
                            : row.status,
                    )}
                    approval={row.approval}
                    documentId={row.reversalId}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            </PreviewSection>
            {posted ? (
                <PreviewNote>
                    已过账记录不可编辑或删除；纠错须追加反向记录。
                </PreviewNote>
            ) : null}
        </div>
    )
}
