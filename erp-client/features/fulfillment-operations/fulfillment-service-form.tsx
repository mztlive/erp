"use client"

import { OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import type {
    FulfillmentDraft,
    FulfillmentResultCode,
    FulfillmentTask,
} from "./types"
import { RESULT_OPTIONS } from "./types"

/** 本地时区 YYYY-MM-DDTHH:mm，用于「填当前时间」快捷填充。 */
function nowLocal(): string {
    const d = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function FulfillmentServiceForm({
    task,
    draft,
    onChange,
    disabled,
}: {
    task: FulfillmentTask
    draft: Extract<FulfillmentDraft, { type: "SERVICE" }>
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return (
        <section className="space-y-3" aria-label="线下服务表单">
            <h3 className="text-sm font-semibold">线下服务</h3>
            <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="service-loc">服务地点</Label>
                    <Input
                        id="service-loc"
                        value={draft.serviceLocation}
                        disabled={disabled}
                        onChange={(e) =>
                            onChange({
                                ...draft,
                                serviceLocation: e.target.value,
                            })
                        }
                    />
                </div>
                <div className="space-y-1.5">
                    <div className="flex items-center justify-between gap-2">
                        <Label htmlFor="service-start">开始时间</Label>
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            disabled={disabled}
                            onClick={() =>
                                onChange({ ...draft, startedAt: nowLocal() })
                            }
                        >
                            填当前时间
                        </Button>
                    </div>
                    <DateTimeLocalPicker
                        value={draft.startedAt || undefined}
                        disabled={disabled}
                        onValueChange={(next) =>
                            onChange({ ...draft, startedAt: next ?? "" })
                        }
                    />
                </div>
                <div className="space-y-1.5">
                    <div className="flex items-center justify-between gap-2">
                        <Label htmlFor="service-ended">结束时间</Label>
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            disabled={disabled}
                            onClick={() =>
                                onChange({ ...draft, endedAt: nowLocal() })
                            }
                        >
                            填当前时间
                        </Button>
                    </div>
                    <DateTimeLocalPicker
                        value={draft.endedAt || undefined}
                        disabled={disabled}
                        onValueChange={(next) =>
                            onChange({ ...draft, endedAt: next ?? "" })
                        }
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="svc-result">履约结果</Label>
                    <OptionCombobox
                        id="svc-result"
                        value={draft.result}
                        onValueChange={(v) =>
                            onChange({
                                ...draft,
                                result: (v ??
                                    draft.result) as FulfillmentResultCode,
                            })
                        }
                        options={RESULT_OPTIONS}
                        allowClear={false}
                        disabled={disabled}
                        aria-label="履约结果"
                        placeholder="请选择履约结果"
                    />
                </div>
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="service-note">完成说明</Label>
                    <Textarea
                        id="service-note"
                        value={draft.completionNote}
                        disabled={disabled}
                        rows={3}
                        onChange={(e) =>
                            onChange({
                                ...draft,
                                completionNote: e.target.value,
                            })
                        }
                    />
                </div>
            </div>
            {draft.lines.map((line, i) => {
                const src = task.lines.find(
                    (l) => l.salesOrderLineId === line.salesOrderLineId,
                )
                return (
                    <div
                        key={line.salesOrderLineId}
                        className="space-y-2 rounded-xl border border-border p-3"
                    >
                        <p className="text-sm font-medium">{src?.itemName}</p>
                        <p className="text-xs text-muted-foreground">
                            对应 {task.source.salesOrderNo}
                        </p>
                        <div className="space-y-1.5">
                            <Label htmlFor={`svc-qty-${i}`}>服务数量</Label>
                            <Input
                                id={`svc-qty-${i}`}
                                className="num"
                                inputMode="decimal"
                                value={line.quantity}
                                disabled={disabled}
                                onChange={(e) => {
                                    const lines = draft.lines.map((l, idx) =>
                                        idx === i
                                            ? { ...l, quantity: e.target.value }
                                            : l,
                                    )
                                    onChange({ ...draft, lines })
                                }}
                            />
                        </div>
                    </div>
                )
            })}
        </section>
    )
}
