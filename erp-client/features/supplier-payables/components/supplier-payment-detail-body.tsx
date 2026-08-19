"use client"

import * as React from "react"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { SupplierPaymentApprovalArea } from "@/features/supplier-payables/components/supplier-payment-approval-area"
import { supplierPaymentApprovalPhase } from "@/features/supplier-payables/lib/supplier-payment-approval"
import type { PaymentRow } from "@/features/supplier-payables/types"
import { formatDateTime } from "@/lib/datetime"

/**
 * 供应商付款详情。草稿展示绑定卡，运行中/终态嵌入通用审批区。
 */
export function SupplierPaymentDetailBody({
    row,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    row: PaymentRow
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const posted = row.status === "POSTED" || row.status === "REVERSED"
    return (
        <div className="space-y-5 overflow-auto p-6">
            {posted ? (
                <Alert variant="info">
                    <AlertTitle>已过账记录只读</AlertTitle>
                    <AlertDescription>
                        已过账记录不可编辑、不可删除；纠错仅能追加冲正。
                    </AlertDescription>
                </Alert>
            ) : null}
            <SupplierPaymentApprovalArea
                phase={supplierPaymentApprovalPhase(
                    row.approval,
                    row.status === "IN_APPROVAL" ? "IN_APPROVAL" : row.status,
                )}
                approval={row.approval}
                documentId={row.paymentId}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                workItemAllowedActions={workItemAllowedActions}
                onDecisionApplied={onDecisionApplied}
            />
            <div className="grid grid-cols-2 gap-3">
                <Fact label="付款单号" value={row.paymentNo} mono />
                <Fact label="供应商" value={row.supplierName} />
                <Fact
                    label="付款时间"
                    value={formatDateTime(row.paidAt, "full", "passthrough")}
                    mono
                />
                <Fact
                    label="付款金额"
                    value={<MoneyValue value={row.amount} taxBasis="gross" />}
                />
                <Fact label="银行引用" value={row.bankReferenceMasked} mono />
                <Fact
                    label="净已分配"
                    value={
                        <MoneyValue
                            value={row.allocatedTotal}
                            taxBasis="gross"
                        />
                    }
                />
                <Fact
                    label="未分配"
                    value={
                        <MoneyValue
                            value={row.unallocatedAmount}
                            taxBasis="gross"
                        />
                    }
                />
            </div>
            <section>
                <h4 className="mb-2 text-sm font-semibold">
                    分配明细（新增不覆盖原金额）
                </h4>
                {row.allocations.length === 0 ? (
                    <p className="text-sm text-muted-foreground">尚无分配行</p>
                ) : (
                    <ul className="space-y-2">
                        {row.allocations.map((allocation) => (
                            <li
                                key={allocation.allocationId}
                                className="rounded-xl border px-3 py-2 text-sm"
                            >
                                <div className="flex justify-between gap-2">
                                    <span>
                                        <Badge
                                            variant={
                                                allocation.action === "REVERSE"
                                                    ? "warning"
                                                    : "secondary"
                                            }
                                        >
                                            {allocation.action === "REVERSE"
                                                ? "反向记录"
                                                : "分配"}
                                        </Badge>{" "}
                                        {allocation.sourceDocumentNo}
                                    </span>
                                    <MoneyValue value={allocation.amount} />
                                </div>
                                <div className="text-xs text-muted-foreground">
                                    {formatDateTime(
                                        allocation.occurredAt,
                                        "full",
                                        "passthrough",
                                    )}
                                    {allocation.reverseOfAllocationId
                                        ? " · 冲减原分配"
                                        : null}
                                </div>
                            </li>
                        ))}
                    </ul>
                )}
            </section>
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
