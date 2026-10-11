"use client"

import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
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
    resultDecisionsForFact,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    FACT_ONLY_NOTICE,
    FULFILLMENT_TYPE_LABEL,
    type AcceptanceOverallResult,
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
    const decisions = resultDecisionsForFact(fact)
    const result = draft?.result ?? "PASS"
    const passed = draft ? passQuantity(draft) : "0"
    const fullException = Boolean(
        draft &&
        result !== "PASS" &&
        ![...selection.selected.values()].some(
            (entry) =>
                entry.fact.salesOrderLineId === fact.salesOrderLineId &&
                isPositiveQty(passQuantity(entry)),
        ),
    )
    const showExceptionNotice = Boolean(draft && hasFilledException(draft))
    const trackingEntries = (fact.trackingEntries ?? []).filter(
        (entry) => entry.salesOrderLineId === fact.salesOrderLineId,
    )
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
                    </p>
                    {trackingEntries.length > 0 ? (
                        <ul className="mt-1 space-y-0.5 text-xs text-muted-foreground">
                            {trackingEntries.map((entry) => (
                                <li
                                    key={`${entry.salesOrderLineId}:${entry.carrier ?? ""}:${entry.trackingNo}`}
                                >
                                    {entry.carrier ? `${entry.carrier} · ` : ""}
                                    <span className="num font-mono">
                                        {entry.trackingNo}
                                    </span>
                                </li>
                            ))}
                        </ul>
                    ) : null}
                </div>
                <p className="text-xs font-medium">
                    待验 {qtyWithUnit(fact.eligibleQuantity, fact.unitCode)}
                </p>
            </div>

            <div className="mt-3 flex items-center gap-2 border-t border-border pt-3">
                <Checkbox
                    id={`sales-orders-acceptance-batch-${segmentId}-include`}
                    checked={Boolean(draft)}
                    disabled={!canPost}
                    onCheckedChange={(checked) => {
                        if (checked) selection.includeFact(fact)
                        else selection.skipFact(fact.fulfillmentLineId)
                    }}
                />
                <Label
                    htmlFor={`sales-orders-acceptance-batch-${segmentId}-include`}
                >
                    纳入本次验收
                </Label>
            </div>

            {draft ? (
                <fieldset className="mt-4 min-w-0">
                    <legend className="mb-2 text-xs font-medium">
                        本批验收结果
                    </legend>
                    <RadioGroup
                        value={result}
                        onValueChange={(value) =>
                            selection.selectResult(
                                fact,
                                value as AcceptanceOverallResult,
                            )
                        }
                        disabled={!canPost}
                        aria-label={`${fact.itemSnapshot} 本批验收结果`}
                        className="grid-cols-1 gap-2 lg:grid-cols-3"
                    >
                        {decisions.map((option) => {
                            const controlId = `sales-orders-acceptance-batch-${segmentId}-result-${toAutomationIdSegment(option)}`
                            const copy = RESULT_COPY[option]
                            return (
                                <Label
                                    key={option}
                                    htmlFor={controlId}
                                    className={cn(
                                        "items-start rounded-lg border p-3 leading-normal transition-colors",
                                        canPost
                                            ? "cursor-pointer"
                                            : "opacity-60",
                                        result === option
                                            ? "border-primary bg-primary/5 ring-1 ring-primary"
                                            : "border-border hover:bg-muted/50",
                                    )}
                                >
                                    <RadioGroupItem
                                        id={controlId}
                                        value={option}
                                        className="mt-0.5"
                                        aria-describedby={`${controlId}-description`}
                                    />
                                    <span className="min-w-0">
                                        <span className="block text-sm">
                                            {copy.label}
                                        </span>
                                        <span
                                            id={`${controlId}-description`}
                                            className="mt-1 block text-xs font-normal text-muted-foreground"
                                        >
                                            {copy.description}
                                        </span>
                                    </span>
                                </Label>
                            )
                        })}
                    </RadioGroup>
                </fieldset>
            ) : (
                <p className="mt-2 text-xs text-muted-foreground" role="status">
                    本批暂不登记，仍保留待验状态。重新勾选后可继续调整。
                </p>
            )}

            {draft && !singlePiece ? (
                <div className="mt-3 flex flex-wrap items-end gap-3">
                    <Field className="w-28">
                        <FieldLabel
                            htmlFor={`sales-orders-acceptance-batch-${segmentId}-qty`}
                        >
                            本次验收数量
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
                </div>
            ) : null}
            {draft ? (
                <p className="mt-3 text-xs font-medium" role="status">
                    本次验收 {qtyWithUnit(draft.qty, fact.unitCode)} · 通过{" "}
                    {qtyWithUnit(passed, fact.unitCode)}
                    {result !== "PASS"
                        ? ` · ${RESULT_COPY[result].quantityLabel} ${qtyWithUnit(draft.exceptionQty, fact.unitCode)}`
                        : ""}
                </p>
            ) : null}
            {fullException ? (
                <div
                    className="mt-3 rounded-lg border border-warning-border bg-warning-soft p-3"
                    role="alert"
                >
                    <p className="text-sm font-medium text-warning-soft-foreground">
                        本明细没有通过数量，暂不能登记验收
                    </p>
                    <p className="mt-1 text-xs text-warning-soft-foreground">
                        请按实际结果核对本明细各批数量。若全部短少、拒收或服务不通过，请暂不登记对应批次，再单独处理异常；当前页面尚不支持创建异常处理单。
                    </p>
                    <Button
                        id={`sales-orders-acceptance-batch-${segmentId}-skip`}
                        type="button"
                        variant="outline"
                        size="sm"
                        className="mt-2"
                        disabled={!canPost}
                        onClick={() =>
                            selection.skipFact(fact.fulfillmentLineId)
                        }
                    >
                        暂不登记本批
                    </Button>
                    <p className="mt-1 text-xs text-muted-foreground">
                        本次弹窗内保留填写内容，重新勾选即可恢复。
                    </p>
                </div>
            ) : showExceptionNotice ? (
                <p className="mt-2 text-xs text-warning-soft-foreground">
                    {FACT_ONLY_NOTICE}
                </p>
            ) : null}
        </article>
    )
}

const RESULT_COPY: Record<
    AcceptanceOverallResult,
    {
        label: string
        description: string
        quantityLabel: string
    }
> = {
    PASS: {
        label: "全部通过",
        description: "本次验收数量全部通过",
        quantityLabel: "通过",
    },
    SHORT: {
        label: "有短少",
        description: "应验商品中有未收到的部分",
        quantityLabel: "短少",
    },
    REJECT: {
        label: "有拒收",
        description: "客户拒绝接收部分或全部商品",
        quantityLabel: "拒收",
    },
    SERVICE_FAIL: {
        label: "服务不通过",
        description: "客户反馈服务未达到要求",
        quantityLabel: "不通过",
    },
}
