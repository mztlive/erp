"use client"

import { MoneyValue } from "@/components/business"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    cents,
    fromCents,
} from "@/features/supplier-payables/lib/allocation-model"

/** 进项发票登记左右分栏工作区底部金额核对栏。 */
export function InvoiceSessionFooter({
    factAmount,
    allocatedHint,
    unallocatedHint,
    isExistingInvoice,
    selectedCount,
    draftHint,
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
    submitHint?: string
    isSubmitting: boolean
    onSaveDraft: () => void
}) {
    const factCents = cents(factAmount || "0")
    const allocatedCents = cents(allocatedHint || "0")
    const overAllocated = factCents > BigInt(0) && allocatedCents > factCents
    const overAmount = overAllocated ? fromCents(allocatedCents - factCents) : "0"

    return (
        <aside
            aria-label="本次进项发票登记"
            className="sticky bottom-0 z-20 mt-auto border-t border-border bg-card/95 py-4 backdrop-blur-xs"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-8 gap-y-4">
                <div className="flex flex-wrap items-center gap-x-8 gap-y-3">
                    <div className="space-y-1">
                        <p className="text-xs text-muted-foreground">
                            {isExistingInvoice ? "本次可核销金额" : "发票金额"}
                        </p>
                        <MoneyValue
                            value={factAmount || "0"}
                            className="text-2xl font-semibold"
                        />
                    </div>
                    <div className="space-y-1 border-l pl-8">
                        <p className="text-xs text-muted-foreground">
                            本次核销
                        </p>
                        <MoneyValue
                            value={allocatedHint}
                            className="text-2xl font-semibold"
                        />
                    </div>
                    <div className="space-y-1 border-l pl-8">
                        <p className="text-xs text-muted-foreground">
                            {overAllocated ? "超出发票金额" : "未核销金额"}
                        </p>
                        <MoneyValue
                            className={
                                overAllocated
                                    ? "text-lg font-semibold text-destructive"
                                    : "text-lg font-medium"
                            }
                            value={
                                overAllocated
                                    ? overAmount
                                    : unallocatedHint
                            }
                        />
                    </div>
                </div>
                <div className="flex flex-wrap items-center justify-end gap-3">
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
            <p
                role={submitHint || overAllocated ? "status" : undefined}
                className={
                    submitHint || overAllocated
                        ? "mt-3 text-xs text-destructive"
                        : "mt-3 text-xs text-muted-foreground"
                }
            >
                {submitHint ??
                    (overAllocated
                        ? "本次核销金额已超出发票金额，请调整后再登记核销。"
                        : (draftHint ??
                          (selectedCount > 0
                              ? `本次已选 ${selectedCount} 笔单据 · 登记后更新收票核销金额。`
                              : "请选择需核销的应付单据。")))}
            </p>
        </aside>
    )
}
