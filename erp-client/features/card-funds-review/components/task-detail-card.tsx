"use client"

import * as React from "react"

import {
    BusinessStatusBadge,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type {
    AllocationDraftLine,
    CardFundsReviewItemView,
    InvoiceDraft,
    ReceiptDraft,
} from "@/features/card-funds-review/types"
import {
    REVIEW_TYPE_LABEL,
    WORK_ITEM_TYPE_LABEL,
} from "@/features/card-funds-review/types"
import { CardFundsAllocationEditor } from "./card-funds-allocation-editor"
import { CardFundsOverview } from "./card-funds-overview"
import { CardFundsRecords } from "./card-funds-records"

/** 当前任务主体卡：对象头 + 概览 + 回款/发票明细 + 登记编辑器。 */
export function TaskDetailCard({
    task,
    headingRef,
    w11Href,
    openAllocation,
    allocationMode,
    receiptForm,
    setReceiptForm,
    invoiceForm,
    setInvoiceForm,
    allocLines,
    setAllocLines,
    allocTarget,
    allocatedSum,
    receiptPending,
    invoicePending,
    setAllocationMode,
    submitReceipt,
    submitInvoice,
}: {
    task: CardFundsReviewItemView
    headingRef: React.RefObject<HTMLHeadingElement | null>
    w11Href: string
    openAllocation: (mode: "receipt" | "invoice") => void
    allocationMode: "receipt" | "invoice" | null
    receiptForm: ReceiptDraft
    setReceiptForm: React.Dispatch<React.SetStateAction<ReceiptDraft>>
    invoiceForm: InvoiceDraft
    setInvoiceForm: React.Dispatch<React.SetStateAction<InvoiceDraft>>
    allocLines: AllocationDraftLine[]
    setAllocLines: React.Dispatch<React.SetStateAction<AllocationDraftLine[]>>
    allocTarget: string
    allocatedSum: string
    receiptPending: boolean
    invoicePending: boolean
    setAllocationMode: React.Dispatch<
        React.SetStateAction<"receipt" | "invoice" | null>
    >
    submitReceipt: () => void | Promise<void>
    submitInvoice: () => void | Promise<void>
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <div className="flex flex-wrap items-center gap-2">
                    <CardTitle>
                        <h2
                            ref={headingRef}
                            tabIndex={-1}
                            className="outline-none"
                            aria-live="polite"
                        >
                            {task.salesOrder.orderNo} ·{" "}
                            {task.account.customerName}
                        </h2>
                    </CardTitle>
                    <BusinessStatusBadge
                        context="list"
                        label={REVIEW_TYPE_LABEL[task.reviewType]}
                        tone={
                            task.reviewType === "OPENING" ? "info" : "warning"
                        }
                    />
                    <Badge variant="secondary">
                        {WORK_ITEM_TYPE_LABEL[task.workItem.workItemType]}
                    </Badge>
                </div>
                <CardDescription>
                    数据版本 r{task.salesOrder.revisionNo} · 同步于{" "}
                    {task.salesOrder.snapshotAt} · 往来{" "}
                    {task.account.counterpartyPartyName}
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
                <CardFundsOverview task={task} />
                <CardFundsRecords
                    task={task}
                    w11Href={w11Href}
                    openAllocation={openAllocation}
                />

                <CardFundsAllocationEditor
                    allocationMode={allocationMode}
                    task={task}
                    receiptForm={receiptForm}
                    setReceiptForm={setReceiptForm}
                    invoiceForm={invoiceForm}
                    setInvoiceForm={setInvoiceForm}
                    allocLines={allocLines}
                    setAllocLines={setAllocLines}
                    allocTarget={allocTarget}
                    allocatedSum={allocatedSum}
                    receiptPending={receiptPending}
                    invoicePending={invoicePending}
                    setAllocationMode={setAllocationMode}
                    submitReceipt={submitReceipt}
                    submitInvoice={submitInvoice}
                />
            </CardContent>
        </Card>
    )
}
