"use client"

import { DocumentSection } from "@/components/business"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { cn } from "@/lib/utils"
import {
    formatOccurredAt,
    parseQty,
    type AcceptanceFactSelection,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    FULFILLMENT_TYPE_LABEL,
    type AcceptanceEligibleFact,
    type AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceFactPool({
    salesLines,
    selected,
    canPost,
    onToggleFact,
    onAllocQtyChange,
}: {
    salesLines: AcceptanceSalesLineGroup[]
    selected: AcceptanceFactSelection
    canPost: boolean
    onToggleFact: (fact: AcceptanceEligibleFact, checked: boolean) => void
    onAllocQtyChange: (fulfillmentLineId: string, qty: string) => void
}) {
    return (
        <DocumentSection
            id="acceptance-fact-pool"
            className="min-w-0 py-0"
            title="可验收的交付记录"
            description="可选一条或多条交付批次验收；同一批次也可以分多次验收。"
        >
            <div className="max-h-[min(32rem,70vh)] space-y-4 overflow-y-auto">
                {salesLines.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        筛选条件下无履约记录。可切换「全部历史记录」或去作业队列查看。
                    </p>
                ) : (
                    salesLines.map((line) => (
                        <section
                            key={line.salesOrderLineId}
                            aria-labelledby={`line-h-${line.salesOrderLineId}`}
                            className="space-y-2"
                        >
                            <h3
                                id={`line-h-${line.salesOrderLineId}`}
                                className="text-sm font-semibold"
                            >
                                明细 {line.lineNo} · {line.itemSnapshot}
                            </h3>
                            <p className="text-xs text-muted-foreground">
                                销售要求 {line.requiredQuantity} {line.unitCode}{" "}
                                · 净已验收 {line.netAcceptedQuantity}{" "}
                                {line.unitCode}
                                <span className="ms-2 text-2xs uppercase tracking-wide opacity-70">
                                    来源：销售单明细 / 交付记录
                                </span>
                            </p>
                            <ul className="space-y-2" role="list">
                                {line.fulfillmentFacts.map((fact) => {
                                    const eligible = parseQty(
                                        fact.eligibleQuantity,
                                    )
                                    const checked = selected.has(
                                        fact.fulfillmentLineId,
                                    )
                                    const disabled = eligible <= 0
                                    return (
                                        <li
                                            key={fact.fulfillmentLineId}
                                            className={cn(
                                                "rounded-lg border border-border px-3 py-2",
                                                checked &&
                                                    "border-primary/50 bg-primary/5",
                                            )}
                                        >
                                            <div className="flex flex-wrap items-start gap-3">
                                                <div className="flex items-center gap-2 pt-0.5">
                                                    <Checkbox
                                                        id={`fact-${fact.fulfillmentLineId}`}
                                                        checked={checked}
                                                        disabled={
                                                            disabled || !canPost
                                                        }
                                                        onCheckedChange={(
                                                            value,
                                                        ) =>
                                                            onToggleFact(
                                                                fact,
                                                                value === true,
                                                            )
                                                        }
                                                        aria-describedby={`fact-meta-${fact.fulfillmentLineId}`}
                                                    />
                                                    <Label
                                                        htmlFor={`fact-${fact.fulfillmentLineId}`}
                                                        className="cursor-pointer font-medium"
                                                    >
                                                        {
                                                            FULFILLMENT_TYPE_LABEL[
                                                                fact
                                                                    .fulfillmentFactType
                                                            ]
                                                        }{" "}
                                                        <span className="num font-mono text-xs">
                                                            {fact.fulfillmentNo}
                                                        </span>
                                                    </Label>
                                                </div>
                                                <div
                                                    id={`fact-meta-${fact.fulfillmentLineId}`}
                                                    className="min-w-0 flex-1 text-xs text-muted-foreground"
                                                >
                                                    <div>
                                                        发生{" "}
                                                        {formatOccurredAt(
                                                            fact.occurredAt,
                                                        )}{" "}
                                                        · 有效履约{" "}
                                                        {
                                                            fact.netSuccessfulQuantity
                                                        }{" "}
                                                        {fact.unitCode} · 已验收{" "}
                                                        {
                                                            fact.netAcceptedAllocatedQuantity
                                                        }{" "}
                                                        {fact.unitCode}
                                                    </div>
                                                    <div className="num mt-0.5 font-medium text-foreground">
                                                        本次最多可验收{" "}
                                                        {fact.eligibleQuantity}{" "}
                                                        {fact.unitCode}
                                                        {disabled
                                                            ? " · 已验完"
                                                            : ""}
                                                    </div>
                                                    {fact.trackingNo ? (
                                                        <div>
                                                            {fact.carrier}{" "}
                                                            {fact.trackingNo}
                                                        </div>
                                                    ) : null}
                                                    <div className="mt-0.5 text-2xs uppercase tracking-wide opacity-70">
                                                        来源：履约记录 ·
                                                        可验收量以系统记录为准
                                                    </div>
                                                </div>
                                                {checked ? (
                                                    <Field className="w-28 shrink-0">
                                                        <FieldLabel
                                                            htmlFor={`alloc-qty-${fact.fulfillmentLineId}`}
                                                        >
                                                            分配
                                                        </FieldLabel>
                                                        <Input
                                                            id={`alloc-qty-${fact.fulfillmentLineId}`}
                                                            className="num"
                                                            inputMode="decimal"
                                                            value={
                                                                selected.get(
                                                                    fact.fulfillmentLineId,
                                                                )?.qty ?? ""
                                                            }
                                                            aria-describedby={`alloc-unit-${fact.fulfillmentLineId}`}
                                                            onChange={(e) =>
                                                                onAllocQtyChange(
                                                                    fact.fulfillmentLineId,
                                                                    e.target
                                                                        .value,
                                                                )
                                                            }
                                                        />
                                                        <FieldDescription
                                                            id={`alloc-unit-${fact.fulfillmentLineId}`}
                                                        >
                                                            {fact.unitCode}
                                                        </FieldDescription>
                                                    </Field>
                                                ) : null}
                                            </div>
                                        </li>
                                    )
                                })}
                            </ul>
                        </section>
                    ))
                )}
            </div>
        </DocumentSection>
    )
}
