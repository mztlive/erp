"use client"

import type { ComponentProps, Dispatch, SetStateAction } from "react"

import {
    BusinessStatusBadge,
    DetailWorkspaceSection,
} from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { LoadingButton } from "@/components/ui/loading-button"
import { formatDateTime } from "@/lib/datetime"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    utf8ByteLength,
} from "@/lib/validation/reason-length"

import type { SupplierOrderCenterResult } from "../hooks/use-supplier-order-center-actions"
import { useSupplierOrderCenterNoteForm } from "../hooks/use-supplier-order-center-forms"
import type { SupplierOrderDetailView } from "../types"

const actionLabels: Record<string, string> = {
    PLACE: "下单",
    QUERY: "查询原结果",
    QUERY_RESULT: "查询原结果",
    REPLAY: "安全重发",
    CANCEL: "取消",
    REFUND: "退款",
    NOTE: "记录说明",
}

const outcomeLabels: Record<string, string> = {
    PENDING: "待发送",
    SENDING: "发送中",
    RESULT_UNKNOWN: "结果未知",
    SUCCEEDED: "成功",
    FAILED: "明确失败",
    MANUAL: "待人工",
}

export function investigationLabel(
    evidence: SupplierOrderDetailView["lastInvestigation"],
) {
    if (!evidence) return "待核实"
    switch (evidence.outcome) {
        case "VERIFIED_TERMINAL":
            return "处理结果已核实"
        case "VERIFIED_NO_RESULT":
            return "已核实无结果"
        case "RESULT_UNKNOWN":
            return "结果仍未知"
    }
}

const resolutionLabels = {
    ORDER_ACCEPTED: "已接单",
    ORDER_REJECTED: "明确拒绝",
    ORDER_COMPLETED: "已完成",
    CANCELED: "已取消",
    REFUNDED: "已退款",
}

export function remoteInvestigationLabel(
    evidence: SupplierOrderDetailView["lastInvestigation"],
) {
    return evidence?.verifiedResolution
        ? resolutionLabels[evidence.verifiedResolution]
        : investigationLabel(evidence)
}

export function InvestigationTime({ value }: { value?: string }) {
    return value ? (
        <time dateTime={value} className="num">
            {formatDateTime(value, "fullIntl", "passthrough")}
        </time>
    ) : (
        "—"
    )
}

export function SupplierInvestigationDiagnostics({
    detail,
    evidence,
    remoteStatus,
    lastSync,
}: {
    detail: SupplierOrderDetailView
    evidence: SupplierOrderDetailView["lastInvestigation"]
    remoteStatus?: string
    lastSync?: string
}) {
    const { order } = detail
    const actions = [...detail.actions].sort((left, right) =>
        right.at.localeCompare(left.at),
    )
    const payloadFacts = [
        ["ERP 订单号", order.orderNo],
        ["供应商单号", evidence?.externalOrderNo ?? order.externalOrderNo],
        ["连接", order.connectionCode],
    ]

    return (
        <>
            <DetailWorkspaceSection
                title="同步诊断"
                className="border-b border-border/60 py-3.5"
                contentClassName="space-y-2"
            >
                <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-xs">
                    <dt className="text-muted-foreground">供应商状态</dt>
                    <dd>{remoteStatus ?? "尚未核实"}</dd>
                    <dt className="text-muted-foreground">ERP 履约</dt>
                    <dd>{order.fulfillmentLabel}</dd>
                    <dt className="text-muted-foreground">调查结论</dt>
                    <dd>{investigationLabel(evidence)}</dd>
                    <dt className="text-muted-foreground">最近同步</dt>
                    <dd>
                        <InvestigationTime value={lastSync} />
                    </dd>
                </dl>
                {evidence?.summary ? (
                    <p className="text-xs">{evidence.summary}</p>
                ) : null}
                {detail.trackingError ? (
                    <p className="text-xs text-warning-soft-foreground">
                        {detail.trackingError}
                    </p>
                ) : null}
                {detail.actionBlockers
                    .filter((blocker) => blocker.action === "QUERY_RESULT")
                    .map((blocker) => (
                        <p
                            key={blocker.code}
                            className="text-xs text-warning-soft-foreground"
                        >
                            {blocker.message}
                        </p>
                    ))}
            </DetailWorkspaceSection>
            <DetailWorkspaceSection
                title="报文摘要"
                className="border-b border-border/60 py-3.5"
                contentClassName="space-y-2"
            >
                <dl className="space-y-1.5 text-xs">
                    {payloadFacts.map(([label, value]) => (
                        <div
                            key={label}
                            className="flex flex-wrap items-baseline gap-x-2"
                        >
                            <dt className="text-muted-foreground">{label}</dt>
                            <dd className="num select-all">{value || "—"}</dd>
                        </div>
                    ))}
                </dl>
                {actions.some((action) => action.techSummary) ? null : (
                    <p className="text-xs text-muted-foreground">
                        暂无接口报文摘要
                    </p>
                )}
            </DetailWorkspaceSection>
            <DetailWorkspaceSection
                title="接口事件"
                className="border-b border-border/60 py-3.5"
                contentClassName="space-y-2"
            >
                {actions.length ? (
                    <ol className="divide-y divide-border/60">
                        {actions.map((action) => (
                            <li
                                key={action.actionId}
                                className="space-y-1 py-2 first:pt-0 last:pb-0"
                            >
                                <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-body-compact">
                                    <span className="font-medium">
                                        {actionLabels[action.actionType] ??
                                            "接口操作"}
                                    </span>
                                    <BusinessStatusBadge
                                        context="list"
                                        tone={action.outcomeTone}
                                        appearance="quiet"
                                        label={
                                            outcomeLabels[
                                                action.outcomeLabel
                                            ] ?? "结果待核实"
                                        }
                                    />
                                </div>
                                <div className="flex flex-wrap gap-x-3 text-xs text-muted-foreground">
                                    <InvestigationTime value={action.at} />
                                    <span>{action.actor}</span>
                                    <span className="num">
                                        尝试 {action.attemptCount} 次
                                    </span>
                                </div>
                                {action.techSummary ? (
                                    <p className="whitespace-pre-wrap wrap-anywhere text-xs">
                                        {action.techSummary}
                                    </p>
                                ) : null}
                            </li>
                        ))}
                    </ol>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        暂无接口事件
                    </p>
                )}
            </DetailWorkspaceSection>
        </>
    )
}

export function SupplierInvestigationNote({
    idPrefix,
    detail,
    noteMutation,
    setResult,
    locked,
}: {
    idPrefix: string
    detail: SupplierOrderDetailView
    noteMutation: Parameters<
        typeof useSupplierOrderCenterNoteForm
    >[0]["noteMutation"]
    setResult: Dispatch<SetStateAction<SupplierOrderCenterResult | null>>
    locked: boolean
}) {
    const form = useSupplierOrderCenterNoteForm({
        orderId: detail.order.id,
        detail,
        noteMutation,
        setResult,
    })

    return (
        <DetailWorkspaceSection title="记录协同说明" className="py-3.5">
            <form
                className="space-y-2"
                onSubmit={(event) => {
                    event.preventDefault()
                    if (
                        !locked &&
                        utf8ByteLength(form.state.values.comment) <=
                            BPM_REASON_MAX_UTF8_BYTES
                    )
                        void form.handleSubmit()
                }}
            >
                <form.AppField
                    name="comment"
                    children={(field) => {
                        const blocked = locked || noteMutation.isPending
                        const error = field.state.meta.isTouched
                            ? field.state.meta.errors[0]?.message
                            : undefined
                        const canSubmit =
                            !blocked &&
                            utf8ByteLength(field.state.value) <=
                                BPM_REASON_MAX_UTF8_BYTES
                        const fieldProps: ComponentProps<
                            typeof ReasonTextareaField
                        > = {
                            id: `${idPrefix}-note`,
                            mode: "utf8-bytes",
                            max: BPM_REASON_MAX_UTF8_BYTES,
                            label: "协同说明",
                            required: true,
                            value: field.state.value,
                            onChange: field.handleChange,
                            onBlur: field.handleBlur,
                            error,
                            disabled: blocked,
                            onCtrlEnterSubmit: canSubmit
                                ? () => void form.handleSubmit()
                                : undefined,
                        }
                        return (
                            <>
                                <ReasonTextareaField {...fieldProps} />
                                <LoadingButton
                                    id={`${idPrefix}-submit-note`}
                                    type="submit"
                                    size="sm"
                                    variant="outline"
                                    loading={noteMutation.isPending}
                                    disabled={!canSubmit}
                                >
                                    提交说明
                                </LoadingButton>
                            </>
                        )
                    }}
                />
            </form>
        </DetailWorkspaceSection>
    )
}
