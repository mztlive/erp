"use client"

import type * as React from "react"

import {
    AllocationWorkspace,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type {
    AllocationDraftLine,
    CardFundsReviewItemView,
} from "@/features/card-funds-review/types"
import { formatMoney, moneyStrSafe } from "../presentation"

type ReceiptDraft = Readonly<{
    receiptNo: string
    receivedAt: string
    grossAmount: string
}>

type InvoiceDraft = Readonly<{
    invoiceNo: string
    issuedAt: string
    grossAmount: string
    netAmount: string
    taxAmount: string
}>

export function CardFundsAllocationEditor({
    allocationMode,
    task,
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
    allocationMode: "receipt" | "invoice" | null
    task: CardFundsReviewItemView
    receiptForm: ReceiptDraft
    setReceiptForm: React.Dispatch<React.SetStateAction<ReceiptDraft>>
    invoiceForm: InvoiceDraft
    setInvoiceForm: React.Dispatch<React.SetStateAction<InvoiceDraft>>
    allocLines: AllocationDraftLine[]
    setAllocLines: React.Dispatch<React.SetStateAction<AllocationDraftLine[]>>
    allocTarget: number
    allocatedSum: number
    receiptPending: boolean
    invoicePending: boolean
    setAllocationMode: React.Dispatch<
        React.SetStateAction<"receipt" | "invoice" | null>
    >
    submitReceipt: () => void | Promise<void>
    submitInvoice: () => void | Promise<void>
}) {
    return (
        <>
            {allocationMode ? (
                <div className="space-y-3">
                    <Card size="sm" className={surfacePanelClassName}>
                        <CardHeader className="border-b border-border/30 py-3">
                            <CardTitle className="text-base">
                                {allocationMode === "receipt"
                                    ? "登记历史回款"
                                    : "登记历史发票"}
                            </CardTitle>
                            <CardDescription>
                                登记为新增分配，不覆盖已有金额；禁止 0 元单据
                            </CardDescription>
                        </CardHeader>
                        <CardContent className="grid gap-3 pt-4 sm:grid-cols-2">
                            {allocationMode === "receipt" ? (
                                <>
                                    <div className="space-y-1.5">
                                        <Label htmlFor="rcpt-no">
                                            回款单号
                                        </Label>
                                        <Input
                                            id="rcpt-no"
                                            value={receiptForm.receiptNo}
                                            onChange={(e) =>
                                                setReceiptForm((f) => ({
                                                    ...f,
                                                    receiptNo: e.target.value,
                                                }))
                                            }
                                            placeholder="可空则系统生成"
                                        />
                                    </div>
                                    <div className="space-y-1.5">
                                        <Label htmlFor="rcpt-amt">
                                            含税金额
                                        </Label>
                                        <Input
                                            id="rcpt-amt"
                                            className="num"
                                            value={receiptForm.grossAmount}
                                            onChange={(e) => {
                                                const grossAmount =
                                                    e.target.value
                                                setReceiptForm((f) => ({
                                                    ...f,
                                                    grossAmount,
                                                }))
                                                setAllocLines((lines) =>
                                                    lines.map((l, i) =>
                                                        i === 0
                                                            ? {
                                                                  ...l,
                                                                  amount:
                                                                      grossAmount ||
                                                                      "0.00",
                                                              }
                                                            : l,
                                                    ),
                                                )
                                            }}
                                            placeholder="须 > 0"
                                        />
                                    </div>
                                    <div className="space-y-1.5">
                                        <Label htmlFor="rcpt-at">
                                            到账日期
                                        </Label>
                                        <DatePicker
                                            value={
                                                receiptForm.receivedAt ||
                                                undefined
                                            }
                                            onValueChange={(next) =>
                                                setReceiptForm((f) => ({
                                                    ...f,
                                                    receivedAt: next ?? "",
                                                }))
                                            }
                                        />
                                    </div>
                                </>
                            ) : (
                                <>
                                    <div className="space-y-1.5">
                                        <Label htmlFor="inv-no">发票号码</Label>
                                        <Input
                                            id="inv-no"
                                            value={invoiceForm.invoiceNo}
                                            onChange={(e) =>
                                                setInvoiceForm((f) => ({
                                                    ...f,
                                                    invoiceNo: e.target.value,
                                                }))
                                            }
                                        />
                                    </div>
                                    <div className="space-y-1.5">
                                        <Label htmlFor="inv-amt">
                                            含税金额
                                        </Label>
                                        <Input
                                            id="inv-amt"
                                            className="num"
                                            value={invoiceForm.grossAmount}
                                            onChange={(e) => {
                                                const grossAmount =
                                                    e.target.value
                                                setInvoiceForm((f) => ({
                                                    ...f,
                                                    grossAmount,
                                                }))
                                                setAllocLines((lines) =>
                                                    lines.map((l, i) =>
                                                        i === 0
                                                            ? {
                                                                  ...l,
                                                                  amount:
                                                                      grossAmount ||
                                                                      "0.00",
                                                              }
                                                            : l,
                                                    ),
                                                )
                                            }}
                                            placeholder="须 > 0"
                                        />
                                    </div>
                                    <div className="space-y-1.5">
                                        <Label htmlFor="inv-at">开票日期</Label>
                                        <DatePicker
                                            value={
                                                invoiceForm.issuedAt ||
                                                undefined
                                            }
                                            onValueChange={(next) =>
                                                setInvoiceForm((f) => ({
                                                    ...f,
                                                    issuedAt: next ?? "",
                                                }))
                                            }
                                        />
                                    </div>
                                </>
                            )}
                        </CardContent>
                    </Card>

                    <AllocationWorkspace
                        title="多对多分配"
                        description="分配合计须等于本次单据含税金额；登记不覆盖已有金额，差额以提交后系统结果为准。"
                        summary={{
                            totalToAllocate: formatMoney(
                                moneyStrSafe(allocTarget),
                            ),
                            allocated: formatMoney(moneyStrSafe(allocatedSum)),
                            difference: formatMoney(
                                moneyStrSafe(allocTarget - allocatedSum),
                            ),
                        }}
                        allocations={allocLines}
                        getRowId={(row) => row.lineId}
                        columns={[
                            {
                                id: "target",
                                header: "分配对象",
                                renderValue: ({ item }) => item.targetLabel,
                                renderEditor: ({ item }) => (
                                    <span className="text-sm">
                                        {item.targetLabel}
                                    </span>
                                ),
                            },
                            {
                                id: "amount",
                                header: "分配金额",
                                numeric: true,
                                align: "end",
                                renderValue: ({ item }) =>
                                    formatMoney(item.amount),
                                renderEditor: ({ item, rowIndex }) => (
                                    <Input
                                        className="num"
                                        value={item.amount}
                                        onChange={(e) => {
                                            const amount = e.target.value
                                            setAllocLines((lines) =>
                                                lines.map((l, i) =>
                                                    i === rowIndex
                                                        ? {
                                                              ...l,
                                                              amount,
                                                          }
                                                        : l,
                                                ),
                                            )
                                        }}
                                    />
                                ),
                            },
                        ]}
                        onAddAllocation={() => {
                            if (!task) return
                            setAllocLines((lines) => [
                                ...lines,
                                {
                                    lineId: `al_${Date.now().toString(36)}`,
                                    targetAccountId: task.account.id,
                                    targetLabel: `${task.salesOrder.orderNo} · 本应收`,
                                    amount: "0.00",
                                },
                            ])
                        }}
                        onRemoveAllocation={(_row, _id, rowIndex) => {
                            setAllocLines((lines) =>
                                lines.length <= 1
                                    ? lines
                                    : lines.filter((_, i) => i !== rowIndex),
                            )
                        }}
                        actions={
                            <>
                                <Button
                                    type="button"
                                    variant="outline"
                                    onClick={() => setAllocationMode(null)}
                                >
                                    取消
                                </Button>
                                <Button
                                    type="button"
                                    disabled={receiptPending || invoicePending}
                                    onClick={() => {
                                        if (allocationMode === "receipt") {
                                            void submitReceipt()
                                        } else {
                                            void submitInvoice()
                                        }
                                    }}
                                >
                                    提交分配
                                </Button>
                            </>
                        }
                    />
                </div>
            ) : null}
        </>
    )
}
