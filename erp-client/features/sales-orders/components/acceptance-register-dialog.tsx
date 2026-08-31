"use client"

import { useEffect, useMemo, useState, type ReactNode } from "react"

import { ValidationSummary, type ValidationIssue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import type { AcceptanceFormApi } from "@/features/sales-orders/hooks/use-acceptance-form"
import type { AcceptanceSelectionApi } from "@/features/sales-orders/hooks/use-acceptance-selection"
import { AcceptanceRegisterBatchRow } from "@/features/sales-orders/components/acceptance-register-batch-row"
import { AcceptanceRegisterLineNav } from "@/features/sales-orders/components/acceptance-register-line-nav"
import {
    isPositiveQty,
    lineAcceptanceHint,
    pendingFactsOf,
    qtyWithUnit,
    registerableSalesLines,
} from "@/features/sales-orders/lib/acceptance-model"
import type { AcceptanceSalesLineGroup } from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceRegisterDialog({
    open,
    form,
    salesLines,
    selection,
    canPost,
    ownerLabel,
    isOwner,
    clientIssues,
    postBlockerMessage,
    pendingCount,
    postPending,
    onOpenChange,
    children,
}: {
    open: boolean
    form: AcceptanceFormApi
    salesLines: AcceptanceSalesLineGroup[]
    selection: AcceptanceSelectionApi
    canPost: boolean
    ownerLabel: string
    isOwner: boolean
    clientIssues: ValidationIssue[]
    postBlockerMessage?: string
    pendingCount: number
    postPending: boolean
    onOpenChange: (open: boolean) => void
    children?: ReactNode
}) {
    const lines = useMemo(
        () => registerableSalesLines(salesLines),
        [salesLines],
    )
    const [activeLineId, setActiveLineId] = useState("")

    useEffect(() => {
        if (!open) return
        setActiveLineId((current) => {
            if (lines.some((line) => line.salesOrderLineId === current)) {
                return current
            }
            return lines[0]?.salesOrderLineId ?? ""
        })
    }, [open, lines])

    const activeLine =
        lines.find((line) => line.salesOrderLineId === activeLineId) ?? lines[0]
    const activeFacts = activeLine
        ? activeLine.fulfillmentFacts.filter((fact) =>
              isPositiveQty(fact.eligibleQuantity),
          )
        : []
    const allPass =
        !selection.hasExceptionResult &&
        selection.selected.size === pendingCount &&
        pendingCount > 0
    const primaryLabel = allPass ? "全部通过并确认" : "确认本次验收"
    const exceptionHint = selection.hasExceptionResult
        ? " · 含短少、拒收或不通过"
        : ""

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="sales-orders-acceptance-register-close"
                className="flex h-[min(90vh,48rem)] max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
                showCloseButton={!postPending}
            >
                <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
                    <DialogTitle>登记客户验收</DialogTitle>
                    <DialogDescription>
                        商品选通过、短少或拒收；服务选通过或不通过。打开时默认全部通过，也可把某批改成这次不验。
                    </DialogDescription>
                </DialogHeader>

                {pendingCount === 0 ? (
                    <p className="px-6 py-4 text-sm text-muted-foreground">
                        当前没有待验收的交付记录。
                    </p>
                ) : (
                    <form
                        id="acceptance-form"
                        className="flex min-h-0 flex-1 flex-col"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <div className="shrink-0 border-b border-border px-6 py-3">
                            <form.AppField name="acceptedAt">
                                {(field) => (
                                    <field.DateTimeField
                                        id="sales-orders-acceptance-accepted-at"
                                        label="客户验收时间"
                                        required
                                        disabled={!canPost}
                                        showTimeZone={false}
                                        className="max-w-sm"
                                    />
                                )}
                            </form.AppField>
                        </div>

                        <div className="flex min-h-0 flex-1 flex-col overflow-hidden md:flex-row">
                            {lines.length > 1 ? (
                                <AcceptanceRegisterLineNav
                                    lines={lines}
                                    activeLineId={
                                        activeLine?.salesOrderLineId ?? ""
                                    }
                                    selected={selection.selected}
                                    onSelect={setActiveLineId}
                                />
                            ) : null}

                            <div
                                id="acceptance-register-list"
                                className="min-h-0 min-w-0 flex-1 overflow-y-auto px-6 py-4"
                            >
                                {children}

                                {!isOwner ? (
                                    <Alert
                                        variant="warning"
                                        role="status"
                                        className="mb-4"
                                    >
                                        <AlertTitle>
                                            由{ownerLabel || "负责销售"}登记
                                        </AlertTitle>
                                        <AlertDescription>
                                            只有本单负责销售可以确认客户验收。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                {activeLine ? (
                                    <section className="flex flex-col gap-3">
                                        <header className="flex flex-col gap-1">
                                            <h3 className="text-sm font-semibold">
                                                明细 {activeLine.lineNo} ·{" "}
                                                {activeLine.itemSnapshot}
                                            </h3>
                                            <p className="text-xs text-muted-foreground">
                                                销售{" "}
                                                {qtyWithUnit(
                                                    activeLine.requiredQuantity,
                                                    activeLine.unitCode,
                                                )}
                                                {" · "}
                                                {
                                                    pendingFactsOf([activeLine])
                                                        .length
                                                }{" "}
                                                批待验
                                            </p>
                                            <p className="text-xs text-muted-foreground">
                                                {lineAcceptanceHint(
                                                    activeLine.fulfillmentFacts,
                                                )}
                                            </p>
                                        </header>
                                        <div className="flex flex-col gap-3">
                                            {activeFacts.map((fact) => (
                                                <AcceptanceRegisterBatchRow
                                                    key={fact.fulfillmentLineId}
                                                    fact={fact}
                                                    selection={selection}
                                                    canPost={canPost}
                                                />
                                            ))}
                                        </div>
                                    </section>
                                ) : null}

                                {clientIssues.length > 0 ? (
                                    <div className="mt-4">
                                        <ValidationSummary
                                            issues={clientIssues}
                                            title={`提交前请处理 ${clientIssues.length} 项`}
                                        />
                                    </div>
                                ) : null}

                                {postBlockerMessage ? (
                                    <p
                                        className="mt-3 text-sm text-destructive"
                                        role="alert"
                                    >
                                        {postBlockerMessage}
                                    </p>
                                ) : null}
                            </div>
                        </div>

                        <div className="shrink-0 border-t border-border px-6 py-3">
                            <form.AppField name="comment">
                                {(field) => (
                                    <field.TextareaField
                                        id="sales-orders-acceptance-comment"
                                        label="内部备注"
                                        placeholder="可不填"
                                        rows={2}
                                        disabled={!canPost}
                                    />
                                )}
                            </form.AppField>
                        </div>
                    </form>
                )}

                <DialogFooter className="shrink-0 border-t border-border px-6 py-4 sm:justify-between">
                    <p className="text-sm text-muted-foreground">
                        已选 {selection.selected.size} / {pendingCount} 批
                        {exceptionHint}
                    </p>
                    <div className="flex flex-wrap justify-end gap-2">
                        <Button
                            id="sales-orders-acceptance-register-cancel"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={postPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-orders-acceptance-register-submit"
                                form="acceptance-form"
                                size="sm"
                                label={postPending ? "提交中…" : primaryLabel}
                                pendingLabel="提交中…"
                                disabled={
                                    !canPost ||
                                    postPending ||
                                    selection.selected.size === 0
                                }
                            />
                        </form.AppForm>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
