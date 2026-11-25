"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    acceptanceConfirmLines,
    visibleAcceptanceNo,
    type AcceptanceBatchSelection,
    type AcceptanceConfirmLine,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    OVERALL_RESULT_LABEL,
    type AcceptanceHistoryItem,
    type AcceptanceOverallResult,
} from "@/features/sales-orders/lib/acceptance-types"
import { utf8ByteLength } from "@/lib/validation/reason-length"

const REVERSE_REASON_MAX_BYTES = 512

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
}) {
    const confirmLines = acceptanceConfirmLines(selected)
    const reverseAcceptanceNo = visibleAcceptanceNo(reverseTarget?.acceptanceNo)

    return (
        <>
            <FormalActionConfirmDialog
                actionVariant="default"
                id="sales-orders-acceptance-confirm"
                open={confirmOpen}
                onOpenChange={onConfirmOpenChange}
                title="确认客户验收"
                actionLabel="登记本次验收"
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
                actionVariant="destructive"
                layout="compact"
                id="sales-orders-acceptance-reverse"
                open={Boolean(reverseTarget)}
                onOpenChange={onReverseOpenChange}
                title={
                    reverseAcceptanceNo
                        ? `冲正 ${reverseAcceptanceNo}？`
                        : "冲正这条验收记录？"
                }
                description="原记录会保留，并增加一条冲正记录；对应批次重新变为待验。"
                actionLabel="冲正"
                confirmLabel="确认冲正"
                cancelLabel="取消"
                fromStatus={{ label: "已确认", tone: "success" }}
                toStatus={{ label: "已冲正", tone: "warning" }}
                formContent={
                    <ReasonTextareaField
                        id="sales-orders-acceptance-reverse-reason"
                        mode="utf8-bytes"
                        max={REVERSE_REASON_MAX_BYTES}
                        label="冲正理由"
                        rows={3}
                        required
                        value={reverseReason}
                        onChange={onReverseReasonChange}
                        placeholder="说明误录原因"
                    />
                }
                confirmDisabled={
                    !reverseReason.trim() ||
                    utf8ByteLength(reverseReason) > REVERSE_REASON_MAX_BYTES
                }
                onConfirm={onConfirmReverse}
            />
        </>
    )
}

export function AcceptanceConfirmSelection({
    lines,
}: {
    lines: readonly AcceptanceConfirmLine[]
}) {
    return (
        <ul
            aria-label="本次验收批次"
            className="max-h-[min(24rem,50dvh)] divide-y divide-border overflow-y-auto rounded-lg border border-border"
        >
            {lines.map((line) => (
                <li
                    key={line.fulfillmentLineId}
                    className="flex items-start justify-between gap-3 px-4 py-3"
                >
                    <div className="min-w-0 space-y-0.5 wrap-anywhere">
                        <p className="font-medium text-foreground">
                            {line.itemLabel}
                        </p>
                        <p className="text-xs text-muted-foreground">
                            {line.fulfillmentLabel}
                        </p>
                        {line.reason ? (
                            <p className="text-xs text-muted-foreground">
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
