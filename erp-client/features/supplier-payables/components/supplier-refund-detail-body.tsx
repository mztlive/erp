"use client"

import * as React from "react"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { SupplierRefundApprovalArea } from "@/features/supplier-payables/components/supplier-refund-approval-area"
import { supplierRefundApprovalPhase } from "@/features/supplier-payables/lib/supplier-refund-approval"
import type { SupplierRefundRow } from "@/features/supplier-payables/types"
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
        <div className="space-y-5 overflow-auto p-6">
            {posted ? (
                <Alert variant="info">
                    <AlertTitle>已过账记录只读</AlertTitle>
                    <AlertDescription>
                        已过账退款不可编辑、不可删除；纠错须追加新的反向记录。
                    </AlertDescription>
                </Alert>
            ) : null}
            <SupplierRefundApprovalArea
                phase={supplierRefundApprovalPhase(
                    row.approval,
                    row.status === "in_approval" ? "IN_APPROVAL" : row.status,
                )}
                approval={row.approval}
                documentId={row.refundId}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                workItemAllowedActions={workItemAllowedActions}
                onDecisionApplied={onDecisionApplied}
            />
            <div className="grid grid-cols-2 gap-3">
                <Fact label="退款单号" value={row.refundNo} mono />
                <Fact
                    label="退款金额"
                    value={<MoneyValue value={row.amount} taxBasis="gross" />}
                />
                <Fact
                    label="退款时间"
                    value={formatDateTime(
                        row.occurredAt,
                        "full",
                        "passthrough",
                    )}
                    mono
                />
                <Fact label="原因说明" value={row.reasonText} />
            </div>
        </div>
    )
}

function Fact({
    label,
    value,
    mono,
}: {
    label: string
    value: React.ReactNode
    mono?: boolean
}) {
    return (
        <div>
            <div className="text-xs text-muted-foreground">{label}</div>
            <div
                className={
                    mono ? "num text-sm font-medium" : "text-sm font-medium"
                }
            >
                {value}
            </div>
        </div>
    )
}
