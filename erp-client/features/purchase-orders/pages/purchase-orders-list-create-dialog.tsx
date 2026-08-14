"use client"

import { surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { CreationBasisSearchCombobox } from "@/features/purchase-orders/components/creation-basis-search-combobox"
import type { PurchaseCreationBasis } from "@/features/purchase-orders/types"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"

function CreationBasisSummary({ basis }: { basis: PurchaseCreationBasis }) {
    return (
        <div
            className={`${surfaceInsetClassName} p-3 text-xs text-muted-foreground`}
        >
            <p className="font-medium text-foreground">拆单键（不可混拼）</p>
            <ul className="mt-1 list-disc space-y-0.5 pl-4">
                <li>销售单 {basis.salesOrderNo}</li>
                <li>供应商 {basis.supplierName}</li>
                <li>
                    类型 {PURCHASE_TYPE_LABEL[basis.purchaseType]} · 履约{" "}
                    {
                        FULFILLMENT_RESPONSIBILITY_LABEL[
                            basis.fulfillmentResponsibility
                        ]
                    }
                </li>
                <li>付款 {basis.paymentTermLabel}</li>
                <li>{basis.lines.length} 条已确认分行</li>
            </ul>
        </div>
    )
}

export type PurchaseOrdersCreateDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    openBases: readonly PurchaseCreationBasis[]
    basisFromUrl: string | null
    selectedBasisId: string
    onSelectedBasisIdChange: (value: string) => void
    createPending: boolean
    onCreate: () => void
}

export function PurchaseOrdersCreateDialog({
    open,
    onOpenChange,
    openBases,
    basisFromUrl,
    selectedBasisId,
    onSelectedBasisIdChange,
    createPending,
    onCreate,
}: PurchaseOrdersCreateDialogProps) {
    const selectedBasis = openBases.find(
        (b) => b.basisId === selectedBasisId,
    )
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>从采购创建依据建单</DialogTitle>
                    <DialogDescription>
                        仅使用采购二次确认产生的创建依据，无需额外建单任务。
                        同一依据上的拆单维度已固定，不可跨销售单或跨供应商合并。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3">
                    {openBases.length === 0 && !basisFromUrl ? (
                        <p className="text-sm text-muted-foreground">
                            当前没有可消费的创建依据。请先在采购二次确认完成确认。
                        </p>
                    ) : (
                        <label className="grid gap-1.5 text-sm">
                            <span>选择创建依据</span>
                            <CreationBasisSearchCombobox
                                className="w-full"
                                value={selectedBasisId}
                                onValueChange={(v) =>
                                    onSelectedBasisIdChange(
                                        v ?? selectedBasisId,
                                    )
                                }
                                allowClear={false}
                                aria-label="选择创建依据"
                                placeholder="选择创建依据"
                            />
                        </label>
                    )}
                    {selectedBasis ? (
                        <CreationBasisSummary basis={selectedBasis} />
                    ) : null}
                </div>
                <DialogFooter>
                    <DialogClose
                        render={<Button type="button" variant="outline" />}
                    >
                        取消
                    </DialogClose>
                    <Button
                        type="button"
                        disabled={!selectedBasisId || createPending}
                        onClick={onCreate}
                    >
                        {createPending ? "创建中…" : "创建草稿并打开"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
