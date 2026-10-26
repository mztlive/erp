"use client"

import { useEffect, useMemo, useState, type ReactNode } from "react"

import { ValidationSummary, type ValidationIssue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { fileUploadText } from "@/lib/ui-text"
import { FileUpload } from "@/components/ui/file-upload"
import {
    Field,
    FieldError,
    FieldLabel,
    FieldDescription,
} from "@/components/ui/field"
import { toFieldErrors } from "@/components/form/utils"
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
    const primaryLabel = "下一步：核对结果"
    const passCount = [...selection.selected.values()].filter(
        (draft) => draft.result === "PASS",
    ).length
    const exceptionCount = selection.selected.size - passCount
    const skippedCount = pendingCount - selection.selected.size

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="sales-orders-acceptance-register-close"
                className="flex h-[min(90vh,48rem)] max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
                showCloseButton={!postPending}
            >
                <DialogHeader className="shrink-0 border-b border-grid px-6 py-4 text-left">
                    <DialogTitle>登记客户验收</DialogTitle>
                    <DialogDescription>
                        默认纳入全部 {pendingCount}{" "}
                        批，结果预填为全部通过。请逐批核对；未验收的批次取消勾选。
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
                        <div className="shrink-0 border-b border-grid px-6 py-3">
                            <form.AppField name="acceptedAt">
                                {(field) => (
                                    <field.DateTimeField
                                        id="sales-orders-acceptance-accepted-at"
                                        label="客户验收时间"
                                        required
                                        disabled={!canPost}
                                        showTimeZone={false}
                                        className="sm:max-w-xl sm:flex-row sm:flex-wrap sm:items-center sm:*:w-auto sm:[&>[data-slot=field-label]]:shrink-0"
                                    />
                                )}
                            </form.AppField>
                        </div>

                        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto md:flex-row md:overflow-hidden">
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
                                className="min-w-0 shrink-0 px-6 py-4 md:min-h-0 md:flex-1 md:overflow-y-auto"
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

                                <div className="mt-5 border-t border-grid pt-4">
                                    <form.AppField name="evidenceFile">
                                        {(field) => (
                                            <Field
                                                data-invalid={
                                                    field.state.meta.errors
                                                        .length > 0 || undefined
                                                }
                                            >
                                                <FieldLabel htmlFor="sales-orders-acceptance-evidence-input">
                                                    签收单凭证{" "}
                                                    <span className="text-destructive">
                                                        *
                                                    </span>
                                                </FieldLabel>
                                                <FileUpload
                                                    selectedDescription={
                                                        fileUploadText.pendingSubmit
                                                    }
                                                    idPrefix="sales-orders-acceptance-evidence"
                                                    accept="application/pdf,image/jpeg,image/png,image/webp,.pdf,.jpg,.jpeg,.png,.webp"
                                                    multiple={false}
                                                    density="compact"
                                                    disabled={
                                                        !canPost || postPending
                                                    }
                                                    label="上传签收单凭证"
                                                    selectedFiles={
                                                        field.state.value
                                                            ? [
                                                                  field.state
                                                                      .value,
                                                              ]
                                                            : []
                                                    }
                                                    onFileRemove={() =>
                                                        field.handleChange(null)
                                                    }
                                                    maxFileSize={
                                                        5 * 1024 * 1024
                                                    }
                                                    description="点击选择图片或 PDF"
                                                    onFilesSelected={(
                                                        files,
                                                    ) => {
                                                        field.handleChange(
                                                            files[0] ?? null,
                                                        )
                                                        field.handleBlur()
                                                    }}
                                                />
                                                <FieldDescription>
                                                    支持
                                                    PDF、JPG、PNG、WebP，文件不超过
                                                    5 MB。
                                                </FieldDescription>
                                                <FieldError
                                                    errors={toFieldErrors(
                                                        field.state.meta.errors,
                                                    )}
                                                />
                                            </Field>
                                        )}
                                    </form.AppField>
                                </div>

                                <details className="mt-4 border-t border-grid pt-3">
                                    <summary
                                        id="sales-orders-acceptance-comment-toggle"
                                        className="w-fit cursor-pointer rounded-sm text-sm text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                    >
                                        添加内部备注（选填）
                                    </summary>
                                    <form.AppField name="comment">
                                        {(field) => (
                                            <field.TextareaField
                                                id="sales-orders-acceptance-comment"
                                                label="内部备注"
                                                className="mt-3"
                                                placeholder="可不填"
                                                rows={2}
                                                disabled={!canPost}
                                            />
                                        )}
                                    </form.AppField>
                                </details>

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
                    </form>
                )}

                <DialogFooter className="shrink-0 border-t border-grid px-6 py-4 sm:justify-between">
                    <div
                        className="text-xs text-muted-foreground"
                        role="status"
                        aria-live="polite"
                    >
                        <p className="text-sm font-medium text-foreground">
                            本次登记 {selection.selected.size} 批
                        </p>
                        <p className="mt-1">
                            全部通过 {passCount} 批 · 有异常 {exceptionCount} 批
                            · 暂不登记 {skippedCount} 批
                        </p>
                    </div>
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
                                loading={postPending}
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
