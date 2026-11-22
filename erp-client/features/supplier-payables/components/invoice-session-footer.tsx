"use client"

import { MoneyValue } from "@/components/business"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    cents,
    fromCents,
} from "@/features/supplier-payables/lib/allocation-model"

/** 进项发票登记左右分栏工作区底部紧凑金额核对栏。 */
export function InvoiceSessionFooter({
    factAmount,
    allocatedHint,
    unallocatedHint,
    isExistingInvoice,
    selectedCount,
    draftHint,
    taxMismatchHint,
    submitHint,
    isSubmitting,
    onSaveDraft,
}: {
    factAmount: string
    allocatedHint: string
    unallocatedHint: string
    isExistingInvoice: boolean
    selectedCount: number
    draftHint?: string | null
    taxMismatchHint?: string
    submitHint?: string
    isSubmitting: boolean
    onSaveDraft: () => void
}) {
    const factCents = cents(factAmount || "0")
    const allocatedCents = cents(allocatedHint || "0")
    const overAllocated = factCents > BigInt(0) && allocatedCents > factCents
    const overAmount = overAllocated
        ? fromCents(allocatedCents - factCents)
        : "0"
    const isErrorHint = Boolean(submitHint || overAllocated || taxMismatchHint)
    const hintText =
        submitHint ??
        (overAllocated
            ? "本次核销金额已超出发票金额"
            : (taxMismatchHint ??
              draftHint ??
              (selectedCount > 0
                  ? `已选 ${selectedCount} 笔单据`
                  : "请选择需核销的应付单据")))

    return (
        <aside
            aria-label="本次进项发票登记"
            className="sticky bottom-0 z-20 mt-auto border-t border-border/70 bg-card/95 px-(--scaffold-px) py-3 backdrop-blur-xs"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
                <div className="flex flex-wrap items-center gap-x-6 gap-y-1.5">
                    <div className="flex items-baseline gap-2">
                        <span className="text-xs text-muted-foreground">
                            {isExistingInvoice ? "可核销" : "发票金额"}
                        </span>
                        <MoneyValue
                            value={factAmount || "0"}
                            className="text-value-compact font-semibold tracking-tight"
                        />
                    </div>
                    <div className="flex items-baseline gap-2 border-l border-border/70 pl-6">
                        <span className="text-xs text-muted-foreground">
                            本次核销
                        </span>
                        <MoneyValue
                            value={allocatedHint}
                            className="text-value-compact font-semibold tracking-tight"
                        />
                    </div>
                    <div className="flex items-baseline gap-2 border-l border-border/70 pl-6">
                        <span className="text-xs text-muted-foreground">
                            {overAllocated ? "超出金额" : "未核销"}
                        </span>
                        <MoneyValue
                            className={
                                overAllocated
                                    ? "text-value-compact font-semibold text-destructive tracking-tight"
                                    : "text-value-compact font-semibold tracking-tight"
                            }
                            value={overAllocated ? overAmount : unallocatedHint}
                        />
                    </div>
                    <span
                        role={isErrorHint ? "status" : undefined}
                        className={
                            isErrorHint
                                ? "border-l border-border/70 pl-6 text-xs text-destructive"
                                : "border-l border-border/70 pl-6 text-xs text-muted-foreground"
                        }
                    >
                        {hintText}
                    </span>
                </div>
                <div className="flex flex-wrap items-center justify-end gap-2.5">
                    <LoadingButton
                        id="supplier-payables-session-page-save-draft"
                        type="button"
                        variant="outline"
                        disabled={isSubmitting}
                        onClick={onSaveDraft}
                    >
                        保存草稿
                    </LoadingButton>
                    <LoadingButton
                        id="supplier-payables-allocation-form-submit"
                        type="submit"
                        loading={isSubmitting}
                        disabled={isSubmitting || isExistingInvoice}
                    >
                        确认登记并核销
                    </LoadingButton>
                </div>
            </div>
        </aside>
    )
}
