"use client"

import { InfoIcon } from "lucide-react"
import { FormalActionConfirmDialog, MoneyValue } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { CustomerReceiptApprovalArea } from "@/features/customer-receivables/components/customer-receipt-approval-area"

/**
 * 客户回款提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function CustomerReceiptSubmitConfirmDialog({
    open,
    pending,
    approval,
    onOpenChange,
    onConfirm,
    id = "customer-receivables-session-receipt-confirm-dialog",
    idPrefix,
    counterpartyName,
    amountLabel,
    amount,
    allocationCount,
    allocatedAmount,
    unallocatedAmount,
}: {
    open: boolean
    pending: boolean
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onConfirm: () => void | Promise<void>
    id?: string
    idPrefix?: string
    counterpartyName: string
    amountLabel: string
    amount: string
    allocationCount: number
    allocatedAmount: string
    unallocatedAmount: string
}) {
    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            layout="compact"
            contentClassName="max-h-[calc(100dvh-2rem)] overflow-y-auto data-[size=default]:max-w-[calc(100%-2rem)] data-[size=default]:sm:max-w-xl"
            id={idPrefix ?? id}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交回款"
            confirmLabel="确认提交"
            fromStatus={{ label: "草稿", tone: "neutral" }}
            toStatus={{ label: "审批中", tone: "warning" }}
            description="请核对回款金额与核销分配，提交后进入审批。"
            formContent={
                <div className="min-w-0 space-y-5">
                    <section
                        aria-label="回款核对"
                        className="overflow-hidden rounded-lg border border-border"
                    >
                        <div className="space-y-4 bg-muted px-5 py-4">
                            <div>
                                <p className="text-xs text-muted-foreground">
                                    结算主体
                                </p>
                                <p className="mt-1 break-words text-sm font-medium">
                                    {counterpartyName}
                                </p>
                            </div>
                            <div>
                                <p className="mb-1 text-xs text-muted-foreground">
                                    {amountLabel}
                                </p>
                                <MoneyValue value={amount} size="hero" />
                            </div>
                        </div>
                        <dl className="grid grid-cols-2 divide-x divide-border border-t border-border">
                            <div className="min-w-0 space-y-1.5 px-5 py-4">
                                <dt className="text-xs text-muted-foreground">
                                    拟核销 · {allocationCount} 笔
                                </dt>
                                <dd className="break-words">
                                    <MoneyValue
                                        value={allocatedAmount}
                                        size="section"
                                    />
                                </dd>
                            </div>
                            <div className="min-w-0 space-y-1.5 px-5 py-4">
                                <dt className="text-xs text-muted-foreground">
                                    剩余待核销
                                </dt>
                                <dd className="break-words">
                                    <MoneyValue
                                        value={unallocatedAmount}
                                        size="section"
                                    />
                                </dd>
                            </div>
                        </dl>
                    </section>
                    <CustomerReceiptApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                    <div className="flex items-start gap-2 border-t border-border pt-4 text-xs leading-relaxed text-muted-foreground">
                        <InfoIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0"
                        />
                        <div className="space-y-1">
                            <p className="font-medium text-foreground">
                                审批全部通过后，回款过账并核销。
                            </p>
                            <p>
                                审批期间不可修改；驳回后重新提交将从首节点审批。
                            </p>
                        </div>
                    </div>
                </div>
            }
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}
