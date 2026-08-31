"use client"

import { CircleSlashIcon } from "lucide-react"

import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { AcceptanceSelectionApi } from "@/features/sales-orders/hooks/use-acceptance-selection"
import {
    exceptionQuantityLabel,
    formatOccurredAt,
    hasFilledException,
    isPositiveQty,
    isSinglePiece,
    passQuantity,
    qtyWithUnit,
    batchDecisionsForFact,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    BATCH_DECISION_LABEL,
    FACT_ONLY_NOTICE,
    FULFILLMENT_TYPE_LABEL,
    SKIP_DECISION,
    type AcceptanceBatchDecision,
    type AcceptanceEligibleFact,
} from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceRegisterBatchRow({
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
    const decision: AcceptanceBatchDecision = draft
        ? draft.result
        : SKIP_DECISION
    const decisions = batchDecisionsForFact(fact)
    const result = draft?.result ?? "PASS"
    const passed = draft ? passQuantity(draft) : "0"
    const fullException = Boolean(
        draft && result !== "PASS" && !isPositiveQty(passed),
    )
    const showExceptionNotice = Boolean(draft && hasFilledException(draft))
    const segmentId = toAutomationIdSegment(fact.fulfillmentLineId)

    return (
        <article className="rounded-lg border border-border bg-card px-3 py-3">
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
                </div>
                <p className="text-xs font-medium">
                    待验 {qtyWithUnit(fact.eligibleQuantity, fact.unitCode)}
                </p>
            </div>

            <ToggleGroup
                value={[decision]}
                onValueChange={(values) => {
                    const next = values[0] as
                        | AcceptanceBatchDecision
                        | undefined
                    if (!next) return
                    if (next === SKIP_DECISION) {
                        selection.skipFact(fact.fulfillmentLineId)
                        return
                    }
                    selection.selectResult(fact, next)
                }}
                variant="outline"
                spacing={0}
                size="sm"
                disabled={!canPost}
                className="mt-3 flex-wrap"
                aria-label={`${fact.itemSnapshot} 本批验收结果`}
            >
                {decisions.map((option) => (
                    <ToggleGroupItem
                        key={option}
                        id={decisionControlId(segmentId, option)}
                        value={option}
                        className="data-[state=on]:bg-foreground data-[state=on]:text-background"
                    >
                        {option === SKIP_DECISION ? (
                            <CircleSlashIcon data-icon="inline-start" />
                        ) : null}
                        {BATCH_DECISION_LABEL[option]}
                    </ToggleGroupItem>
                ))}
            </ToggleGroup>

            {!draft ? (
                <p className="mt-2 text-xs text-muted-foreground">
                    本批不计入这次验收。需要验收时再选结果。
                </p>
            ) : null}

            {draft && !singlePiece ? (
                <div className="mt-3 flex flex-wrap items-end gap-3">
                    <Field className="w-28">
                        <FieldLabel
                            htmlFor={`sales-orders-acceptance-batch-${segmentId}-qty`}
                        >
                            本次数量
                        </FieldLabel>
                        <Input
                            id={`sales-orders-acceptance-batch-${segmentId}-qty`}
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
                                htmlFor={`sales-orders-acceptance-batch-${segmentId}-exception-qty`}
                            >
                                {exceptionQuantityLabel(result)}
                            </FieldLabel>
                            <Input
                                id={`sales-orders-acceptance-batch-${segmentId}-exception-qty`}
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
                <div className="mt-3 flex flex-col gap-2">
                    <Field>
                        <FieldLabel
                            htmlFor={`sales-orders-acceptance-batch-${segmentId}-reason`}
                        >
                            客户反馈
                        </FieldLabel>
                        <Textarea
                            id={`sales-orders-acceptance-batch-${segmentId}-reason`}
                            rows={2}
                            value={draft.reason}
                            disabled={!canPost}
                            placeholder={
                                result === "SERVICE_FAIL"
                                    ? "服务不通过时必填"
                                    : "短少或拒收时必填"
                            }
                            onChange={(event) =>
                                selection.updateDraft(fact.fulfillmentLineId, {
                                    reason: event.target.value,
                                })
                            }
                        />
                    </Field>
                    {fullException ? (
                        <p className="text-xs text-warning-soft-foreground">
                            整件短少或拒收不能记入验收。请改选「本次不验」，再另开退货处理，或多件时减少短少数量并保留通过。
                        </p>
                    ) : showExceptionNotice ? (
                        <p className="text-xs text-warning-soft-foreground">
                            {FACT_ONLY_NOTICE}
                        </p>
                    ) : null}
                </div>
            ) : null}
        </article>
    )
}

function decisionControlId(
    segmentId: string,
    option: AcceptanceBatchDecision,
): string {
    if (option === SKIP_DECISION) {
        return `sales-orders-acceptance-batch-${segmentId}-skip`
    }
    return `sales-orders-acceptance-batch-${segmentId}-result-${toAutomationIdSegment(option)}`
}
