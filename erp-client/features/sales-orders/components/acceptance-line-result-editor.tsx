"use client"

import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import type { AcceptanceFactSelection, LineResultState } from "@/features/sales-orders/lib/acceptance-model"
import type { AcceptanceEligibleFact } from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceLineResultEditor({
    lineId,
    facts,
    result,
    unit,
    hasService,
    canPost,
    selected,
    onUpdate,
}: {
    lineId: string
    facts: AcceptanceEligibleFact[]
    result: LineResultState
    unit: string
    hasService: boolean
    canPost: boolean
    selected: AcceptanceFactSelection
    onUpdate: (salesOrderLineId: string, patch: Partial<LineResultState>) => void
}) {
    return (
        <fieldset
            id={`line-result-${lineId}`}
            className="space-y-3 rounded-lg border border-border p-3"
        >
            <legend className="px-1 text-sm font-semibold">
                {facts[0]?.itemSnapshot ?? lineId}
            </legend>
            <p className="text-xs text-muted-foreground">
                来源分配：
                {facts
                    .map(
                        (f) =>
                            `${f.fulfillmentNo} ${selected.get(f.fulfillmentLineId)?.qty ?? 0}`,
                    )
                    .join(" · ")}
            </p>
            <div className="grid gap-3 sm:grid-cols-3">
                <Field>
                    <FieldLabel htmlFor={`acc-${lineId}`}>
                        通过数量
                    </FieldLabel>
                    <Input
                        id={`acc-${lineId}`}
                        className="num"
                        inputMode="decimal"
                        value={result.acceptedQuantity}
                        disabled={!canPost}
                        onChange={(e) =>
                            onUpdate(lineId, {
                                acceptedQuantity: e.target.value,
                            })
                        }
                    />
                    <FieldDescription>{unit}</FieldDescription>
                </Field>
                <Field>
                    <FieldLabel htmlFor={`short-${lineId}`}>
                        短少数量
                    </FieldLabel>
                    <Input
                        id={`short-${lineId}`}
                        className="num"
                        inputMode="decimal"
                        value={result.shortQuantity}
                        disabled={!canPost}
                        onChange={(e) =>
                            onUpdate(lineId, {
                                shortQuantity: e.target.value,
                            })
                        }
                    />
                </Field>
                <Field>
                    <FieldLabel htmlFor={`rej-${lineId}`}>
                        {hasService ? "拒收/不通过" : "拒收数量"}
                    </FieldLabel>
                    <Input
                        id={`rej-${lineId}`}
                        className="num"
                        inputMode="decimal"
                        value={result.rejectedQuantity}
                        disabled={!canPost}
                        onChange={(e) =>
                            onUpdate(lineId, {
                                rejectedQuantity: e.target.value,
                            })
                        }
                    />
                </Field>
            </div>
            {hasService ? (
                <label className="flex items-center gap-2 text-sm">
                    <Checkbox
                        checked={result.serviceFail}
                        disabled={!canPost}
                        onCheckedChange={(v) =>
                            onUpdate(lineId, {
                                serviceFail: v === true,
                            })
                        }
                    />
                    标记为服务不通过
                </label>
            ) : null}
            <Field>
                <FieldLabel htmlFor={`line-reason-${lineId}`}>
                    客户反馈 / 原因
                </FieldLabel>
                <Textarea
                    id={`line-reason-${lineId}`}
                    rows={2}
                    value={result.reason}
                    disabled={!canPost}
                    placeholder="短少、拒收或服务不通过时必填"
                    onChange={(e) =>
                        onUpdate(lineId, {
                            reason: e.target.value,
                        })
                    }
                />
            </Field>
        </fieldset>
    )
}
