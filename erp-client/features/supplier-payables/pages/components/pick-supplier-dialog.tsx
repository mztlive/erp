"use client"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import type { AllocationTrack } from "@/features/supplier-payables/types"

export interface PickSupplierDialogProps {
    track: AllocationTrack | null
    supplierId: string
    onSupplierIdChange: (id: string) => void
    onClose: () => void
    onConfirm: () => void
}

export function PickSupplierDialog({
    track,
    supplierId,
    onSupplierIdChange,
    onClose,
    onConfirm,
}: PickSupplierDialogProps) {
    return (
        <Dialog
            open={track != null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
        >
            <DialogContent closeButtonId="supplier-payables-pick-supplier-close">
                <DialogHeader>
                    <DialogTitle>
                        {track === "payment"
                            ? "选择供应商 · 登记付款"
                            : "选择供应商 · 登记进项发票"}
                    </DialogTitle>
                    <DialogDescription>
                        本次核销创建后锁定供应商；不同供应商目标不会进入同一核销池。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-2">
                    <Label>供应商</Label>
                    <SupplierSearchCombobox
                        id="supplier-payables-pick-supplier-select"
                        value={supplierId || undefined}
                        onValueChange={(id) => onSupplierIdChange(id ?? "")}
                        className="w-full"
                        aria-label="供应商"
                        placeholder="选择供应商"
                    />
                </div>
                <DialogFooter>
                    <Button
                        id="supplier-payables-pick-supplier-cancel"
                        type="button"
                        variant="outline"
                        onClick={onClose}
                    >
                        取消
                    </Button>
                    <Button
                        id="supplier-payables-pick-supplier-confirm"
                        type="button"
                        disabled={!supplierId || !track}
                        onClick={() => {
                            if (!track || !supplierId) return
                            onConfirm()
                        }}
                    >
                        进入本次核销
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
