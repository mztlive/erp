"use client"

import * as React from "react"
import Link from "next/link"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PaymentReversalApprovalArea } from "@/features/supplier-payables/components/payment-reversal-approval-area"
import { useSupplierPaymentQuery } from "@/features/supplier-payables/hooks/queries"
import { paymentPreviewHref } from "@/features/supplier-payables/lib/related-documents"
import { paymentReversalApprovalPhase } from "@/features/supplier-payables/lib/payment-reversal-approval"
import type { PaymentReversalRow } from "@/features/supplier-payables/types"
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
        <div className="space-y-5 overflow-auto p-6">
            {posted ? (
                <Alert variant="info">
                    <AlertTitle>已过账记录只读</AlertTitle>
                    <AlertDescription>
                        已过账冲正不可编辑、不可删除；纠错须追加新的反向记录。
                    </AlertDescription>
                </Alert>
            ) : null}
            <PaymentReversalApprovalArea
                phase={paymentReversalApprovalPhase(
                    row.approval,
                    row.status === "in_approval" ? "IN_APPROVAL" : row.status,
                )}
                approval={row.approval}
                documentId={row.reversalId}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                workItemAllowedActions={workItemAllowedActions}
                onDecisionApplied={onDecisionApplied}
            />
            <div className="grid grid-cols-2 gap-3">
                <Fact label="冲正单号" value={row.reversalNo} mono />
                <Fact
                    label="冲正金额"
                    value={<MoneyValue value={row.amount} taxBasis="gross" />}
                />
                <Fact
                    label="冲正时间"
                    value={formatDateTime(
                        row.occurredAt,
                        "full",
                        "passthrough",
                    )}
                    mono
                />
                <Fact label="原因说明" value={row.reasonText} />
                <Fact
                    label="原付款单"
                    value={
                        originalPaymentQuery.isPending
                            ? "正在读取…"
                            : (originalPayment?.paymentNo ?? "付款信息待补全")
                    }
                    mono={Boolean(originalPayment?.paymentNo)}
                />
                <Fact
                    label="供应商"
                    value={originalPayment?.supplierName ?? "供应商信息待补全"}
                />
                {originalPayment ? (
                    <Fact
                        label="原付款金额"
                        value={
                            <MoneyValue
                                value={originalPayment.amount}
                                taxBasis="gross"
                            />
                        }
                    />
                ) : null}
                <div>
                    <Button
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
                </div>
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
