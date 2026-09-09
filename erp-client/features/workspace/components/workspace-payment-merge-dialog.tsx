"use client"

import * as React from "react"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"

import {
    canConfirmMergeSelection,
    defaultMergeSelectedIds,
    selectedMergeOpenTotal,
    type PaymentMergeTask,
} from "../lib/workspace-payment-merge"

export function WorkspacePaymentMergeDialog({
    open,
    items,
    supplierName,
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    items: readonly PaymentMergeTask[]
    supplierName?: string
    onOpenChange: (open: boolean) => void
    onConfirm: (selectedPayableIds: ReadonlySet<string>) => void
}) {
    const [selected, setSelected] = React.useState<Set<string>>(() => new Set())

    React.useEffect(() => {
        if (open) {
            setSelected(defaultMergeSelectedIds(items))
        }
    }, [items, open])

    const total = selectedMergeOpenTotal(selected, items)
    const canConfirm = canConfirmMergeSelection(selected, items)
    const selectedCount = items.filter((item) =>
        selected.has(item.payableAccountId),
    ).length

    function toggleItem(item: PaymentMergeTask, checked: boolean) {
        if (item.isAnchor) return
        setSelected((prev) => {
            const next = new Set(prev)
            if (checked) next.add(item.payableAccountId)
            else next.delete(item.payableAccountId)
            return next
        })
    }

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-w-lg"
                closeButtonId="workspace-payment-merge-close"
            >
                <DialogHeader>
                    <DialogTitle>合并付款</DialogTitle>
                    <DialogDescription>
                        {supplierName
                            ? `将 ${supplierName} 的多笔待付合成一次打款。当前任务必须保留，可取消其他采购单。`
                            : "将同一收款人的多笔待付合成一次打款。当前任务必须保留，可取消其他采购单。"}
                    </DialogDescription>
                </DialogHeader>
                <ul className="max-h-80 space-y-2 overflow-auto">
                    {items.map((item) => {
                        const checked = selected.has(item.payableAccountId)
                        const rowId = `workspace-payment-merge-item-${toAutomationIdSegment(item.payableAccountId)}`
                        return (
                            <li
                                key={item.workItemId}
                                className="flex items-start gap-3 rounded-lg border border-border p-3"
                            >
                                <Checkbox
                                    id={`${rowId}-select`}
                                    checked={checked}
                                    disabled={item.isAnchor}
                                    onCheckedChange={(value) =>
                                        toggleItem(item, value === true)
                                    }
                                />
                                <label
                                    htmlFor={`${rowId}-select`}
                                    className="min-w-0 flex-1 cursor-pointer text-sm"
                                >
                                    <div className="flex items-center justify-between gap-3">
                                        <span className="font-medium">
                                            {item.sourceDocumentNo ?? "采购单"}
                                            {item.isAnchor ? " · 当前任务" : ""}
                                        </span>
                                        <MoneyValue
                                            value={item.openTotal}
                                            taxBasis="gross"
                                        />
                                    </div>
                                    {item.dueDate ? (
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            到期 {item.dueDate}
                                        </p>
                                    ) : null}
                                </label>
                            </li>
                        )
                    })}
                </ul>
                <DialogFooter className="flex-col gap-3 sm:flex-col">
                    <p className="text-sm text-muted-foreground">
                        已选 {selectedCount} 笔，合计{" "}
                        <MoneyValue value={total} taxBasis="gross" />
                    </p>
                    {!canConfirm ? (
                        <p className="text-xs text-muted-foreground">
                            请至少再勾选一笔待付任务，或关闭后按单笔付款。
                        </p>
                    ) : null}
                    <div className="flex w-full justify-end gap-2">
                        <Button
                            id="workspace-payment-merge-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="workspace-payment-merge-confirm"
                            type="button"
                            disabled={!canConfirm}
                            onClick={() => onConfirm(selected)}
                        >
                            开始合并付款
                        </Button>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
