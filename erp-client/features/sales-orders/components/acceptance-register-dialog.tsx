"use client"

import { useEffect, useMemo, useState, type ReactNode } from "react"
import { ArrowRightIcon } from "lucide-react"

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
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"
import type { AcceptanceFormApi } from "@/features/sales-orders/hooks/use-acceptance-form"
import type { AcceptanceSelectionApi } from "@/features/sales-orders/hooks/use-acceptance-selection"
import { AcceptanceConfirmSelection } from "@/features/sales-orders/components/acceptance-dialogs"
import { AcceptanceRegisterBatchRow } from "@/features/sales-orders/components/acceptance-register-batch-row"
import { AcceptanceRegisterLineNav } from "@/features/sales-orders/components/acceptance-register-line-nav"
import {
    acceptanceConfirmLines,
    isPositiveQty,
    pendingFactsOf,
    qtyWithUnit,
    registerableSalesLines,
    type AcceptanceBatchSelection,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    OVERALL_RESULT_LABEL,
    type AcceptanceOverallResult,
    type AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"

export type AcceptanceRegisterFormBodyProps = {
    form: AcceptanceFormApi
    salesLines: AcceptanceSalesLineGroup[]
    selection: AcceptanceSelectionApi
    canPost: boolean
    ownerLabel: string
    isOwner: boolean
    clientIssues: ValidationIssue[]
    postBlockerMessage?: string
    postPending: boolean
    /** true 时作为工作台主画布内联表单渲染；false 时用于弹窗体。 */
    inline?: boolean
    activeKey?: string
    children?: ReactNode
}

/** 客户验收可编辑登记表单主体：工作台主画布内联态与独立销售单弹窗态共用。 */
export function AcceptanceRegisterFormBody({
    form,
    salesLines,
    selection,
    canPost,
    ownerLabel,
    isOwner,
    clientIssues,
    postBlockerMessage,
    postPending,
    inline = false,
    activeKey,
    children,
}: AcceptanceRegisterFormBodyProps) {
    const lines = useMemo(
        () => registerableSalesLines(salesLines),
        [salesLines],
    )
    const [activeLineId, setActiveLineId] = useState("")

    useEffect(() => {
        setActiveLineId((current) => {
            if (lines.some((line) => line.salesOrderLineId === current)) {
                return current
            }
            return lines[0]?.salesOrderLineId ?? ""
        })
    }, [activeKey, lines])

    const activeLine =
        lines.find((line) => line.salesOrderLineId === activeLineId) ?? lines[0]
    const activeFacts = activeLine
        ? activeLine.fulfillmentFacts.filter((fact) =>
              isPositiveQty(fact.eligibleQuantity),
          )
        : []

    return (
        <form
            id="acceptance-form"
            className={
                inline
                    ? "flex min-w-0 flex-col gap-5"
                    : "flex min-h-0 flex-1 flex-col"
            }
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <div
                className={
                    inline
                        ? "border-b border-border/60 pb-4"
                        : "shrink-0 border-b border-grid px-overlay-padding py-3"
                }
            >
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

            <div
                className={
                    inline
                        ? "flex min-w-0 flex-col gap-5 md:flex-row md:items-start"
                        : "flex min-h-0 flex-1 flex-col overflow-y-auto md:flex-row md:overflow-hidden"
                }
            >
                {lines.length > 1 ? (
                    <AcceptanceRegisterLineNav
                        lines={lines}
                        activeLineId={activeLine?.salesOrderLineId ?? ""}
                        selected={selection.selected}
                        onSelect={setActiveLineId}
                    />
                ) : null}

                <div
                    id="acceptance-register-list"
                    className={
                        inline
                            ? "min-w-0 flex-1"
                            : "min-w-0 shrink-0 px-overlay-padding py-4 md:min-h-0 md:flex-1 md:overflow-y-auto"
                    }
                >
                    {children}

                    {!isOwner ? (
                        <Alert variant="warning" role="status" className="mb-4">
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
                                <h3 className="font-semibold">
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
                                    {pendingFactsOf([activeLine]).length} 批待验
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
                                        field.state.meta.errors.length > 0 ||
                                        undefined
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
                                        disabled={!canPost || postPending}
                                        label="上传签收单凭证"
                                        selectedFiles={
                                            field.state.value
                                                ? [field.state.value]
                                                : []
                                        }
                                        onFileRemove={() =>
                                            field.handleChange(null)
                                        }
                                        maxFileSize={5 * 1024 * 1024}
                                        description="点击选择图片或 PDF"
                                        onFilesSelected={(files) => {
                                            field.handleChange(files[0] ?? null)
                                            field.handleBlur()
                                        }}
                                    />
                                    <FieldDescription>
                                        支持 PDF、JPG、PNG、WebP，文件不超过 5
                                        MB。
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
                            className="w-fit cursor-pointer rounded-sm text-body-compact text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
                            className="mt-3 text-body-compact text-destructive"
                            role="alert"
                        >
                            {postBlockerMessage}
                        </p>
                    ) : null}
                </div>
            </div>
        </form>
    )
}

export function AcceptanceRegisterDialog({
    open,
    step,
    onStepChange,
    form,
    salesLines,
    selection,
    confirmSelected,
    overallPreview,
    canPost,
    ownerLabel,
    isOwner,
    clientIssues,
    postBlockerMessage,
    pendingCount,
    postPending,
    onConfirmAcceptance,
    onOpenChange,
    children,
}: {
    open: boolean
    step: "edit" | "confirm"
    onStepChange: (next: "edit" | "confirm") => void
    form: AcceptanceFormApi
    salesLines: AcceptanceSalesLineGroup[]
    selection: AcceptanceSelectionApi
    confirmSelected: AcceptanceBatchSelection
    overallPreview: AcceptanceOverallResult
    canPost: boolean
    ownerLabel: string
    isOwner: boolean
    clientIssues: ValidationIssue[]
    postBlockerMessage?: string
    pendingCount: number
    postPending: boolean
    onConfirmAcceptance: () => Promise<void>
    onOpenChange: (open: boolean) => void
    children?: ReactNode
}) {
    const [confirmError, setConfirmError] = useState<string | null>(null)
    const [submittingConfirm, setSubmittingConfirm] = useState(false)

    useEffect(() => {
        if (!open || step === "edit") {
            setConfirmError(null)
        }
    }, [open, step])

    const busy = postPending || submittingConfirm
    const passCount = [...selection.selected.values()].filter(
        (draft) => draft.result === "PASS",
    ).length
    const exceptionCount = selection.selected.size - passCount
    const skippedCount = pendingCount - selection.selected.size
    const confirmLines = acceptanceConfirmLines(confirmSelected)
    const overallTone =
        overallPreview === "PASS"
            ? "success"
            : overallPreview === "SHORT"
              ? "warning"
              : "destructive"
    const formValues = form.state.values

    const handleConfirmSubmit = async () => {
        if (busy) return
        setSubmittingConfirm(true)
        setConfirmError(null)
        try {
            await onConfirmAcceptance()
        } catch (error) {
            setConfirmError(getErrorMessage(error, "操作未完成，请稍后重试。"))
        } finally {
            setSubmittingConfirm(false)
        }
    }

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && busy) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="2xl"
                scroll="body"
                closeButtonId="sales-orders-acceptance-register-close"
                showCloseButton={!busy}
            >
                <DialogHeader>
                    <DialogTitle>
                        {step === "confirm" ? "确认客户验收" : "登记客户验收"}
                    </DialogTitle>
                    <DialogDescription>
                        {step === "confirm"
                            ? `核对下面 ${confirmLines.length.toLocaleString("zh-CN")} 个批次的验收结果，确认后提交登记。`
                            : `默认纳入全部 ${pendingCount} 批，结果预填为全部通过。请逐批核对；未验收的批次取消勾选。`}
                    </DialogDescription>
                </DialogHeader>

                {pendingCount === 0 ? (
                    <DialogBody>
                        <p className="text-xs text-muted-foreground">
                            当前没有待验收的交付记录。
                        </p>
                    </DialogBody>
                ) : step === "confirm" ? (
                    <DialogBody className="space-y-4">
                        {children}
                        <section
                            aria-label="状态变化"
                            className="flex flex-wrap items-center gap-2"
                        >
                            <StatusBadge label="待登记" tone="warning" />
                            <ArrowRightIcon
                                aria-label="变更为"
                                className="size-4 text-muted-foreground"
                            />
                            <StatusBadge
                                label={OVERALL_RESULT_LABEL[overallPreview]}
                                tone={overallTone}
                            />
                        </section>
                        <dl className="grid grid-cols-1 gap-x-6 gap-y-2 rounded-lg border border-border/80 bg-muted/40 px-4 py-3 text-xs sm:grid-cols-2">
                            <div className="flex items-baseline gap-2">
                                <dt className="shrink-0 text-muted-foreground">
                                    客户验收时间
                                </dt>
                                <dd className="num min-w-0 truncate text-foreground">
                                    {formValues.acceptedAt || "当前时间"}
                                </dd>
                            </div>
                            <div className="flex items-baseline gap-2">
                                <dt className="shrink-0 text-muted-foreground">
                                    签收单凭证
                                </dt>
                                <dd className="min-w-0 truncate text-foreground">
                                    {formValues.evidenceFile?.name ?? "已选择"}
                                </dd>
                            </div>
                            {formValues.comment.trim() ? (
                                <div className="flex items-baseline gap-2 sm:col-span-2">
                                    <dt className="shrink-0 text-muted-foreground">
                                        内部备注
                                    </dt>
                                    <dd className="min-w-0 break-words text-foreground">
                                        {formValues.comment.trim()}
                                    </dd>
                                </div>
                            ) : null}
                        </dl>
                        <AcceptanceConfirmSelection lines={confirmLines} />
                        {confirmError ? (
                            <Alert variant="destructive" role="alert">
                                <AlertTitle>提交失败</AlertTitle>
                                <AlertDescription>
                                    {confirmError}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </DialogBody>
                ) : (
                    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                        <AcceptanceRegisterFormBody
                            form={form}
                            salesLines={salesLines}
                            selection={selection}
                            canPost={canPost}
                            ownerLabel={ownerLabel}
                            isOwner={isOwner}
                            clientIssues={clientIssues}
                            postBlockerMessage={postBlockerMessage}
                            postPending={busy}
                            activeKey={open ? "open" : "closed"}
                        >
                            {children}
                        </AcceptanceRegisterFormBody>
                    </div>
                )}

                <DialogFooter className="sm:items-center sm:justify-between">
                    <div
                        className="text-xs text-muted-foreground"
                        role="status"
                        aria-live="polite"
                    >
                        <p className="text-body-compact font-medium text-foreground">
                            本次登记 {selection.selected.size} 批
                        </p>
                        <p className="mt-1">
                            全部通过 {passCount} 批 · 有异常 {exceptionCount} 批
                            · 暂不登记 {skippedCount} 批
                        </p>
                    </div>
                    <div className="flex flex-wrap justify-end gap-2">
                        {step === "confirm" ? (
                            <>
                                <Button
                                    id="sales-orders-acceptance-confirm-back"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={busy}
                                    onClick={() => onStepChange("edit")}
                                >
                                    返回修改
                                </Button>
                                <LoadingButton
                                    id="sales-orders-acceptance-confirm-submit"
                                    type="button"
                                    size="sm"
                                    loading={busy}
                                    disabled={busy || confirmLines.length === 0}
                                    onClick={() => void handleConfirmSubmit()}
                                >
                                    {busy ? "提交中…" : "确认本次验收"}
                                </LoadingButton>
                            </>
                        ) : (
                            <>
                                <Button
                                    id="sales-orders-acceptance-register-cancel"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={busy}
                                    onClick={() => onOpenChange(false)}
                                >
                                    取消
                                </Button>
                                <form.AppForm>
                                    <form.SubmitButton
                                        id="sales-orders-acceptance-register-submit"
                                        form="acceptance-form"
                                        size="sm"
                                        label={
                                            busy
                                                ? "提交中…"
                                                : "下一步：核对结果"
                                        }
                                        pendingLabel="提交中…"
                                        loading={busy}
                                        disabled={
                                            !canPost ||
                                            busy ||
                                            selection.selected.size === 0
                                        }
                                    />
                                </form.AppForm>
                            </>
                        )}
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
