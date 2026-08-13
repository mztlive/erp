"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { DialogScrollBody } from "@/features/master-data/components/shared/action-dialog-shared"
import { FixedResourceDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { WAREHOUSE_WRITE_MESSAGE } from "@/features/master-data/lib/data"
import type { RevisionTarget } from "@/features/master-data/lib/revision-target"

export function WarehouseDisableDialog({
    open,
    onOpenChange,
    target,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
}) {
    const stockNote =
        target &&
        "warehouseStockSummary" in target &&
        target.warehouseStockSummary?.hasBlockingStock
            ? ` 另：在库 ${target.warehouseStockSummary.onHandQty} / 预占 ${target.warehouseStockSummary.reservedQty} 时也不可停用。`
            : ""
    return (
        <FixedResourceDisableDialog
            resource="warehouses"
            open={open}
            onOpenChange={onOpenChange}
            target={target}
            submitDisabled
            submitLabel={masterDataCopy.createSubmitRejected}
            blockedBanner={
                <Alert variant="destructive">
                    <AlertTitle>
                        {masterDataCopy.warehouseWriteTitle}
                    </AlertTitle>
                    <AlertDescription>
                        {WAREHOUSE_WRITE_MESSAGE}
                        {stockNote}
                    </AlertDescription>
                </Alert>
            }
        />
    )
}

/** 仓库写门禁未开放：只展示阻断，不进入可提交表单。 */
export function WarehouseReviseDialog({
    open,
    onOpenChange,
    target,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>{masterDataCopy.reviseTitle}</DialogTitle>
                    <DialogDescription>
                        {masterDataCopy.reviseDesc}
                        {target ? (
                            <>
                                {" "}
                                资料编号{" "}
                                <span className="num">{target.stableNo}</span>
                            </>
                        ) : null}
                    </DialogDescription>
                </DialogHeader>
                <DialogScrollBody>
                    <Alert variant="destructive">
                        <AlertTitle>
                            {masterDataCopy.warehouseWriteTitle}
                        </AlertTitle>
                        <AlertDescription>
                            {WAREHOUSE_WRITE_MESSAGE}
                        </AlertDescription>
                    </Alert>
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button type="button" variant="outline" />
                            }
                        >
                            关闭
                        </DialogClose>
                        <Button type="button" disabled>
                            {masterDataCopy.createSubmitRejected}
                        </Button>
                    </DialogFooter>
                </DialogScrollBody>
            </DialogContent>
        </Dialog>
    )
}
