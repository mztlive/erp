"use client"

import type { ReactNode } from "react"

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
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { AcceptanceFormApi } from "@/features/sales-orders/hooks/use-acceptance-form"
import type { AcceptanceSelectionApi } from "@/features/sales-orders/hooks/use-acceptance-selection"
import {
    formatOccurredAt,
    hasFilledException,
    isSinglePiece,
    isPositiveQty,
    passQuantity,
    qtyWithUnit,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    FACT_ONLY_NOTICE,
    FULFILLMENT_TYPE_LABEL,
    OVERALL_RESULT_LABEL,
    type AcceptanceEligibleFact,
    type AcceptanceOverallResult,
    type AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"

const RESULT_OPTIONS: AcceptanceOverallResult[] = [
    "PASS",
    "SHORT",
    "REJECT",
    "SERVICE_FAIL",
]

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
    const allPass =
        !selection.hasExceptionResult &&
        selection.selected.size === pendingCount &&
        pendingCount > 0
    const primaryLabel = allPass ? "全部通过并确认" : "确认本次验收"

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="sales-orders-acceptance-register-close"
                className="flex max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl"
                showCloseButton={!postPending}
            >
                <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
                    <DialogTitle>登记客户验收</DialogTitle>
                    <DialogDescription>
                        每批选择通过、短少或拒收。打开时默认全部通过。
                    </DialogDescription>
                </DialogHeader>

                <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
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

                    {pendingCount === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            当前没有待验收的交付记录。
                        </p>
                    ) : (
                        <form
                            id="acceptance-form"
                            className="space-y-4"
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            <form.AppField name="acceptedAt">
                                {(field) => (
                                    <field.DateTimeField
                                        id="sales-orders-acceptance-accepted-at"
                                        label="客户验收时间"
                                        required
                                        disabled={!canPost}
                                        showTimeZone={false}
                                    />
                                )}
                            </form.AppField>

                            <div
                                id="acceptance-register-list"
                                className="space-y-4"
                            >
                                {salesLines.map((line) => {
                                    const facts = line.fulfillmentFacts.filter(
                                        (fact) =>
                                            isPositiveQty(
                                                fact.eligibleQuantity,
                                            ),
                                    )
                                    if (facts.length === 0) return null
                                    return (
                                        <section
                                            key={line.salesOrderLineId}
                                            className="space-y-2"
                                        >
                                            <h3 className="text-sm font-semibold">
                                                明细 {line.lineNo} ·{" "}
                                                {line.itemSnapshot}
                                            </h3>
                                            <p className="text-xs text-muted-foreground">
                                                销售{" "}
                                                {qtyWithUnit(
                                                    line.requiredQuantity,
                                                    line.unitCode,
                                                )}
                                            </p>
                                            <ul className="space-y-2">
                                                {facts.map((fact) => (
                                                    <BatchRow
                                                        key={
                                                            fact.fulfillmentLineId
                                                        }
                                                        fact={fact}
                                                        selection={selection}
                                                        canPost={canPost}
                                                    />
                                                ))}
                                            </ul>
                                        </section>
                                    )
                                })}
                            </div>

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
                        </form>
                    )}

                    {clientIssues.length > 0 ? (
                        <ValidationSummary
                            issues={clientIssues}
                            title={`提交前请处理 ${clientIssues.length} 项`}
                        />
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

                <DialogFooter className="shrink-0 border-t border-border px-6 py-4 sm:justify-between">
                    <p className="text-sm text-muted-foreground">
                        已选 {selection.selected.size} / {pendingCount} 批
                        {selection.hasExceptionResult ? " · 含短少或拒收" : ""}
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
                        <Button
                            id="sales-orders-acceptance-register-submit"
                            type="submit"
                            form="acceptance-form"
                            size="sm"
                            disabled={
                                !canPost ||
                                postPending ||
                                selection.selected.size === 0
                            }
                        >
                            {postPending ? "提交中…" : primaryLabel}
                        </Button>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function BatchRow({
    fact,
    selection,
    canPost,
}: {
    fact: AcceptanceEligibleFact
    selection: AcceptanceSelectionApi
    canPost: boolean
}) {
    const draft = selection.selected.get(fact.fulfillmentLineId)
    const singlePiece = isSinglePiece(fact.eligibleQuantity)
    const result = draft?.result ?? "PASS"
    const resultOptions =
        fact.fulfillmentFactType === "SERVICE"
            ? RESULT_OPTIONS
            : RESULT_OPTIONS.filter((option) => option !== "SERVICE_FAIL")
    const passed = draft ? passQuantity(draft) : "0"
    const fullException = Boolean(
        draft && result !== "PASS" && !isPositiveQty(passed),
    )
    const showExceptionNotice = Boolean(draft && hasFilledException(draft))

    return (
        <li
            className={cn(
                "rounded-lg border px-3 py-3",
                !draft && "border-border bg-card",
                draft?.result === "PASS" && "border-success-border bg-card",
                draft?.result === "SHORT" &&
                    "border-warning-border bg-warning-soft",
                (draft?.result === "REJECT" ||
                    draft?.result === "SERVICE_FAIL") &&
                    "border-destructive-border bg-destructive-soft",
            )}
        >
            <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="min-w-0">
                    <p className="text-sm font-medium">
                        {FULFILLMENT_TYPE_LABEL[fact.fulfillmentFactType]}{" "}
                        <span className="num font-mono text-xs font-normal">
                            {fact.fulfillmentNo}
                        </span>
                    </p>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        {formatOccurredAt(fact.occurredAt)}
                        {fact.trackingNo
                            ? ` · ${fact.carrier ?? ""} ${fact.trackingNo}`
                            : ""}
                    </p>
                    <p className="mt-0.5 text-xs font-medium">
                        待验 {qtyWithUnit(fact.eligibleQuantity, fact.unitCode)}
                    </p>
                </div>
                {draft ? (
                    <Button
                        id={`sales-orders-acceptance-batch-${toAutomationIdSegment(fact.fulfillmentLineId)}-skip`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        disabled={!canPost}
                        onClick={() =>
                            selection.skipFact(fact.fulfillmentLineId)
                        }
                    >
                        本次不验
                    </Button>
                ) : null}
            </div>

            <div className="mt-3 flex flex-wrap gap-1.5">
                {resultOptions.map((option) => {
                    const selected = Boolean(draft) && result === option
                    return (
                        <Button
                            key={option}
                            id={`sales-orders-acceptance-batch-${toAutomationIdSegment(fact.fulfillmentLineId)}-result-${toAutomationIdSegment(option)}`}
                            type="button"
                            size="sm"
                            variant={selected ? "default" : "outline"}
                            disabled={!canPost}
                            aria-pressed={selected}
                            className={cn(
                                selected &&
                                    option === "PASS" &&
                                    "border-transparent bg-success text-success-foreground hover:bg-success/90",
                                selected &&
                                    option === "SHORT" &&
                                    "border-transparent bg-warning text-warning-foreground hover:bg-warning/90",
                                selected &&
                                    (option === "REJECT" ||
                                        option === "SERVICE_FAIL") &&
                                    "border-transparent bg-destructive text-destructive-foreground hover:bg-destructive/90",
                            )}
                            onClick={() => selection.selectResult(fact, option)}
                        >
                            {OVERALL_RESULT_LABEL[option]}
                        </Button>
                    )
                })}
            </div>

            {!draft ? (
                <p className="mt-2 text-xs text-muted-foreground">
                    不计入本次验收。点通过、短少或拒收即可加入。
                </p>
            ) : null}

            {draft && !singlePiece ? (
                <div className="mt-3 flex flex-wrap items-end gap-3">
                    <Field className="w-28">
                        <FieldLabel
                            htmlFor={`sales-orders-acceptance-batch-${toAutomationIdSegment(fact.fulfillmentLineId)}-qty`}
                        >
                            本次数量
                        </FieldLabel>
                        <Input
                            id={`sales-orders-acceptance-batch-${toAutomationIdSegment(fact.fulfillmentLineId)}-qty`}
                            className="num"
                            inputMode="decimal"
                            value={draft.qty}
                            disabled={!canPost}
                            onChange={(event) =>
                                selection.updateDraft(fact.fulfillmentLineId, {
                                    qty: event.target.value,
                                })
                            }
                        />
                    </Field>
                    {result !== "PASS" ? (
                        <Field className="w-28">
                            <FieldLabel
                                htmlFor={`sales-orders-acceptance-batch-${toAutomationIdSegment(fact.fulfillmentLineId)}-exception-qty`}
                            >
                                {result === "SHORT" ? "短少数量" : "拒收数量"}
                            </FieldLabel>
                            <Input
                                id={`sales-orders-acceptance-batch-${toAutomationIdSegment(fact.fulfillmentLineId)}-exception-qty`}
                                className="num"
                                inputMode="decimal"
                                value={draft.exceptionQty}
                                disabled={!canPost}
                                onChange={(event) =>
                                    selection.updateDraft(
                                        fact.fulfillmentLineId,
                                        { exceptionQty: event.target.value },
                                    )
                                }
                            />
                        </Field>
                    ) : null}
                    <p className="pb-2 text-xs text-muted-foreground">
                        通过 {qtyWithUnit(passed, fact.unitCode)}
                    </p>
                </div>
            ) : null}

            {draft && result !== "PASS" ? (
                <div className="mt-3 space-y-2">
                    <Field>
                        <FieldLabel
                            htmlFor={`sales-orders-acceptance-batch-${toAutomationIdSegment(fact.fulfillmentLineId)}-reason`}
                        >
                            客户反馈
                        </FieldLabel>
                        <Textarea
                            id={`sales-orders-acceptance-batch-${toAutomationIdSegment(fact.fulfillmentLineId)}-reason`}
                            rows={2}
                            value={draft.reason}
                            disabled={!canPost}
                            placeholder="短少或拒收时必填"
                            onChange={(event) =>
                                selection.updateDraft(fact.fulfillmentLineId, {
                                    reason: event.target.value,
                                })
                            }
                        />
                    </Field>
                    {fullException ? (
                        <p className="text-xs text-warning-soft-foreground">
                            整件短少或拒收不能记入验收。请点「本次不验」，再另开退货处理。
                        </p>
                    ) : showExceptionNotice ? (
                        <p className="text-xs text-warning-soft-foreground">
                            {FACT_ONLY_NOTICE}
                        </p>
                    ) : null}
                </div>
            ) : null}
        </li>
    )
}
