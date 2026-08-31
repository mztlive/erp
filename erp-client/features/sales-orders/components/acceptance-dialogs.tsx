"use client"

import { RotateCcwIcon } from "lucide-react"

import {
    DiscardConfirmDialog,
    FormalActionConfirmDialog,
} from "@/components/business"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { StatusBadge } from "@/components/ui/status-badge"
import { Textarea } from "@/components/ui/textarea"
import {
    acceptanceConfirmLines,
    type AcceptanceBatchSelection,
    type AcceptanceConfirmLine,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    OVERALL_RESULT_LABEL,
    type AcceptanceHistoryItem,
    type AcceptanceOverallResult,
} from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceDialogs({
    confirmOpen,
    onConfirmOpenChange,
    selected,
    overallPreview,
    onConfirmAcceptance,
    reverseTarget,
    onReverseOpenChange,
    reverseReason,
    onReverseReasonChange,
    onConfirmReverse,
    exitDiscardOpen,
    onExitDiscardOpenChange,
    onConfirmExit,
}: {
    confirmOpen: boolean
    onConfirmOpenChange: (open: boolean) => void
    selected: AcceptanceBatchSelection
    overallPreview: AcceptanceOverallResult
    onConfirmAcceptance: () => Promise<void>
    reverseTarget: AcceptanceHistoryItem | null
    onReverseOpenChange: (open: boolean) => void
    reverseReason: string
    onReverseReasonChange: (value: string) => void
    onConfirmReverse: () => Promise<void>
    exitDiscardOpen: boolean
    onExitDiscardOpenChange: (open: boolean) => void
    onConfirmExit: () => void
}) {
    const confirmLines = acceptanceConfirmLines(selected)

    return (
        <>
            <FormalActionConfirmDialog
                id="sales-orders-acceptance-confirm"
                open={confirmOpen}
                onOpenChange={onConfirmOpenChange}
                title="确认客户验收"
                actionLabel="确认本次验收"
                confirmLabel="确认本次验收"
                fromStatus={{ label: "待登记", tone: "warning" }}
                toStatus={{
                    label: OVERALL_RESULT_LABEL[overallPreview],
                    tone:
                        overallPreview === "PASS"
                            ? "success"
                            : overallPreview === "SHORT"
                              ? "warning"
                              : "destructive",
                }}
                description={`核对下面 ${confirmLines.length.toLocaleString("zh-CN")} 个批次的验收结果。`}
                formContent={
                    <AcceptanceConfirmSelection lines={confirmLines} />
                }
                onConfirm={onConfirmAcceptance}
            />

            <FormalActionConfirmDialog
                id="sales-orders-acceptance-reverse"
                open={Boolean(reverseTarget)}
                onOpenChange={onReverseOpenChange}
                title="冲正错误验收记录？"
                actionLabel="冲正"
                confirmLabel="确认冲正"
                icon={RotateCcwIcon}
                mediaClassName="bg-warning-soft text-warning-soft-foreground"
                fromStatus={{ label: "已确认", tone: "success" }}
                toStatus={{ label: "已冲正", tone: "warning" }}
                lockedFields={[
                    `原验收单 ${reverseTarget?.acceptanceNo ?? "—"}`,
                ]}
                formContent={
                    <Field>
                        <FieldLabel htmlFor="sales-orders-acceptance-reverse-reason">
                            冲正理由 <span className="text-destructive">*</span>
                        </FieldLabel>
                        <Textarea
                            id="sales-orders-acceptance-reverse-reason"
                            rows={3}
                            required
                            value={reverseReason}
                            onChange={(event) =>
                                onReverseReasonChange(event.target.value)
                            }
                            placeholder="说明误录原因"
                        />
                        <FieldDescription>
                            请说明误录原因；该说明将随冲正记录保留。
                        </FieldDescription>
                        {!reverseReason.trim() ? (
                            <p className="text-xs text-destructive" role="alert">
                                请填写冲正理由
                            </p>
                        ) : null}
                    </Field>
                }
                effects={[
                    "新增反向验收记录",
                    "恢复对应批次的待验数量",
                    "不删除原验收记录",
                ]}
                nextDepartment="销售"
                confirmDisabled={!reverseReason.trim()}
                onConfirm={onConfirmReverse}
            />

            <DiscardConfirmDialog
                id="sales-orders-acceptance-exit-discard"
                open={exitDiscardOpen}
                onOpenChange={onExitDiscardOpenChange}
                title="放弃本次验收登记？"
                description="已勾选的批次和填写的结果还没提交，取消后会丢掉。"
                confirmLabel="放弃并返回"
                cancelLabel="继续登记"
                onConfirm={onConfirmExit}
            />
        </>
    )
}

function AcceptanceConfirmSelection({
    lines,
}: {
    lines: readonly AcceptanceConfirmLine[]
}) {
    return (
        <ul
            aria-label="本次验收批次"
            className="max-h-[min(24rem,50vh)] divide-y divide-border overflow-y-auto rounded-xl border border-border"
        >
            {lines.map((line) => (
                <li
                    key={line.fulfillmentLineId}
                    className="flex items-start justify-between gap-3 px-4 py-3"
                >
                    <div className="min-w-0 space-y-0.5">
                        <p className="font-medium text-foreground">
                            {line.itemLabel}
                        </p>
                        <p className="text-sm text-muted-foreground">
                            {line.fulfillmentLabel}
                        </p>
                        {line.reason ? (
                            <p className="text-sm text-muted-foreground">
                                {line.reason}
                            </p>
                        ) : null}
                    </div>
                    <StatusBadge
                        className="shrink-0"
                        tone={line.resultTone}
                        label={line.resultText}
                    />
                </li>
            ))}
        </ul>
    )
}
