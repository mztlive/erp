"use client"

import type { ReactNode } from "react"

import { ValidationSummary, type ValidationIssue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"
import type { AcceptanceFormApi } from "@/features/sales-orders/hooks/use-acceptance-form"
import type { AcceptanceSelectionApi } from "@/features/sales-orders/hooks/use-acceptance-selection"
import {
    formatOccurredAt,
    parseQty,
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
    onPassAll,
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
    onPassAll: () => void
    children?: ReactNode
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="flex max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl"
                showCloseButton={!postPending}
            >
                <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
                    <DialogTitle>登记客户验收</DialogTitle>
                    <DialogDescription>
                        勾选要确认的交付批次。默认全部通过；有短少或拒收再改结果。
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
                                        label="客户验收时间"
                                        required
                                        disabled={!canPost}
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
                                            parseQty(fact.eligibleQuantity) > 0,
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

                            {selection.hasExceptionResult ? (
                                <Alert variant="warning" role="status">
                                    <AlertTitle>
                                        {
                                            OVERALL_RESULT_LABEL[
                                                selection.overallPreview
                                            ]
                                        }
                                    </AlertTitle>
                                    <AlertDescription>
                                        {FACT_ONLY_NOTICE}
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            <form.AppField name="comment">
                                {(field) => (
                                    <field.TextareaField
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
                        已选 {selection.selected.size} / {pendingCount} 批 ·{" "}
                        {OVERALL_RESULT_LABEL[selection.overallPreview]}
                        {selection.hasExceptionResult ? " · 异常只记账" : ""}
                    </p>
                    <div className="flex flex-wrap justify-end gap-2">
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={postPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={
                                !canPost || pendingCount === 0 || postPending
                            }
                            onClick={onPassAll}
                        >
                            本次待验全部通过
                        </Button>
                        <Button
                            type="submit"
                            form="acceptance-form"
                            size="sm"
                            disabled={
                                !canPost ||
                                postPending ||
                                selection.selected.size === 0
                            }
                        >
                            {postPending ? "提交中…" : "确认本次验收"}
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
    const checked = Boolean(draft)
    const result = draft?.result ?? "PASS"
    const resultOptions =
        fact.fulfillmentFactType === "SERVICE"
            ? RESULT_OPTIONS
            : RESULT_OPTIONS.filter((option) => option !== "SERVICE_FAIL")

    return (
        <li
            className={cn(
                "rounded-lg border border-border px-3 py-2",
                checked && "border-primary/50 bg-primary/5",
            )}
        >
            <div className="flex flex-wrap items-start gap-3">
                <div className="flex items-center gap-2 pt-0.5">
                    <Checkbox
                        id={`fact-${fact.fulfillmentLineId}`}
                        checked={checked}
                        disabled={!canPost}
                        onCheckedChange={(value) =>
                            selection.toggleFact(fact, value === true)
                        }
                    />
                    <Label
                        htmlFor={`fact-${fact.fulfillmentLineId}`}
                        className="cursor-pointer font-medium"
                    >
                        {FULFILLMENT_TYPE_LABEL[fact.fulfillmentFactType]}{" "}
                        <span className="num font-mono text-xs">
                            {fact.fulfillmentNo}
                        </span>
                    </Label>
                </div>
                <div className="min-w-0 flex-1 text-xs text-muted-foreground">
                    {formatOccurredAt(fact.occurredAt)}
                    {fact.trackingNo
                        ? ` · ${fact.carrier ?? ""} ${fact.trackingNo}`
                        : ""}
                    <div className="mt-0.5 font-medium text-foreground">
                        待验 {qtyWithUnit(fact.eligibleQuantity, fact.unitCode)}
                    </div>
                </div>
            </div>

            {draft ? (
                <div className="mt-3 space-y-3">
                    <div className="flex flex-wrap items-end gap-3">
                        <Field className="w-28">
                            <FieldLabel
                                htmlFor={`batch-qty-${fact.fulfillmentLineId}`}
                            >
                                本次数量
                            </FieldLabel>
                            <Input
                                id={`batch-qty-${fact.fulfillmentLineId}`}
                                className="num"
                                inputMode="decimal"
                                value={draft.qty}
                                disabled={!canPost}
                                onChange={(event) =>
                                    selection.updateDraft(
                                        fact.fulfillmentLineId,
                                        {
                                            qty: event.target.value,
                                        },
                                    )
                                }
                            />
                        </Field>
                        <div className="flex flex-wrap gap-1">
                            {resultOptions.map((option) => (
                                <Button
                                    key={option}
                                    type="button"
                                    size="sm"
                                    variant={
                                        result === option
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    disabled={!canPost}
                                    onClick={() =>
                                        selection.updateDraft(
                                            fact.fulfillmentLineId,
                                            { result: option },
                                        )
                                    }
                                >
                                    {OVERALL_RESULT_LABEL[option]}
                                </Button>
                            ))}
                        </div>
                        <p className="text-xs text-muted-foreground">
                            通过{" "}
                            {qtyWithUnit(passQuantity(draft), fact.unitCode)}
                        </p>
                    </div>

                    {result !== "PASS" ? (
                        <div className="grid gap-3 sm:grid-cols-2">
                            <Field>
                                <FieldLabel
                                    htmlFor={`batch-exc-${fact.fulfillmentLineId}`}
                                >
                                    {result === "SHORT"
                                        ? "短少数量"
                                        : "拒收数量"}
                                </FieldLabel>
                                <Input
                                    id={`batch-exc-${fact.fulfillmentLineId}`}
                                    className="num"
                                    inputMode="decimal"
                                    value={draft.exceptionQty}
                                    disabled={!canPost}
                                    onChange={(event) =>
                                        selection.updateDraft(
                                            fact.fulfillmentLineId,
                                            {
                                                exceptionQty:
                                                    event.target.value,
                                            },
                                        )
                                    }
                                />
                            </Field>
                            <Field className="sm:col-span-2">
                                <FieldLabel
                                    htmlFor={`batch-reason-${fact.fulfillmentLineId}`}
                                >
                                    客户反馈
                                </FieldLabel>
                                <Textarea
                                    id={`batch-reason-${fact.fulfillmentLineId}`}
                                    rows={2}
                                    value={draft.reason}
                                    disabled={!canPost}
                                    placeholder="短少、拒收或服务不通过时必填"
                                    onChange={(event) =>
                                        selection.updateDraft(
                                            fact.fulfillmentLineId,
                                            { reason: event.target.value },
                                        )
                                    }
                                />
                            </Field>
                        </div>
                    ) : null}
                </div>
            ) : null}
        </li>
    )
}
