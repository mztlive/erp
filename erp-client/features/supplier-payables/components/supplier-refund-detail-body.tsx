"use client"

import Link from "next/link"
import { SupplierSourceDocuments } from "./supplier-source-documents"

import {
    PreviewAmount,
    PreviewSection,
    PreviewNote,
} from "@/components/business/financial-preview"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { SupplierRefundApprovalArea } from "@/features/supplier-payables/components/supplier-refund-approval-area"
import { paymentPreviewHref } from "@/features/supplier-payables/lib/related-documents"
import { supplierRefundApprovalPhase } from "@/features/supplier-payables/lib/supplier-refund-approval"
import type { SupplierRefundRow } from "@/features/supplier-payables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"

/**
 * 供应商退款详情。草稿展示绑定卡，运行中/终态嵌入通用审批区。
 */
export function SupplierRefundDetailBody({
    row,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    row: SupplierRefundRow
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const posted = row.status === "posted" || row.status === "reversed"
    return (
        <div className="min-h-0 flex-1 space-y-6 overflow-auto px-7 py-6 text-sm">
            <PreviewAmount label="退款金额" value={row.amount} />
            <PreviewSection title="原因说明">
                <p className="leading-6">{row.reasonText || "未填写原因"}</p>
                <PreviewNote>
                    退款时间：
                    {formatDateTime(row.occurredAt, "full", "passthrough")}
                </PreviewNote>
            </PreviewSection>
            {row.originalPaymentId ? (
                <PreviewSection title="原付款">
                    {" "}
                    <Button
                        id={`supplier-payables-refund-detail-${toAutomationIdSegment(row.refundId)}-open-original`}
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
                        查看原付款
                    </Button>
                </PreviewSection>
            ) : null}
            {!row.originalPaymentId ? (
                <SupplierSourceDocuments
                    scope={{
                        entryId: row.originalPayableEntryId,
                        supplierId: row.supplierId,
                    }}
                />
            ) : null}
            <PreviewSection title="审批记录">
                <SupplierRefundApprovalArea
                    phase={supplierRefundApprovalPhase(
                        row.approval,
                        row.status === "in_approval"
                            ? "IN_APPROVAL"
                            : row.status,
                    )}
                    approval={row.approval}
                    documentId={row.refundId}
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
