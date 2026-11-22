import type * as React from "react"
import Link from "next/link"
import {
    CheckCircle2Icon,
    ChevronDownIcon,
    ExternalLinkIcon,
    RefreshCwIcon,
    ShieldAlertIcon,
} from "lucide-react"

import { BusinessDiffPanel, BusinessStatusBadge } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasAuthenticationRecovery } from "../lib/presentation"
import type { IntegrationResolutionItemView } from "../types"
import { DIFFERENCE_TYPE_LABEL } from "../types"

export function IntegrationItemSummary({
    item,
    headingRef,
    onRefresh,
    flat,
}: {
    item: IntegrationResolutionItemView
    headingRef: React.RefObject<HTMLHeadingElement | null>
    onRefresh: () => void
    flat?: boolean
}) {
    const recovered = hasAuthenticationRecovery(item)
    const terminal =
        item.workItem?.status === "COMPLETED" ||
        item.workItem?.status === "CLOSED"
    const authentication =
        item.classification.errorClass === "authentication-or-signature"
    const failure = item.failureSource
    const error = failure?.summary?.match(/^([A-Z][A-Z0-9_]+):\s*([\s\S]*)$/)
    const failureMessage = error?.[2] || failure?.summary

    return (
        <section aria-labelledby="integration-item-title" className="space-y-5">
            <div className="flex items-start justify-between gap-4">
                <div className="min-w-0 space-y-2">
                    <h3
                        id="integration-item-title"
                        ref={headingRef}
                        tabIndex={-1}
                        className={
                            flat
                                ? "scroll-mt-32 break-words text-section-title font-semibold tracking-tight outline-none"
                                : "break-words text-body-compact font-semibold outline-none"
                        }
                    >
                        {[item.identity.number, item.businessObject.title]
                            .filter(Boolean)
                            .join(" · ")}
                    </h3>
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>处理人：{item.ownerUser ?? item.ownerRole}</span>
                        <span>已滞留 {item.ageLabel}</span>
                        <span>{item.fundsImpactLabel}</span>
                        {item.environment ? (
                            <span>{item.environmentLabel}环境</span>
                        ) : null}
                    </div>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                    {!flat && (
                        <BusinessStatusBadge
                            context="detail"
                            label={item.status.label}
                            tone={
                                item.workItem?.status === "CLOSED"
                                    ? "neutral"
                                    : terminal
                                      ? "success"
                                      : "info"
                            }
                        />
                    )}
                    <Button
                        id="integration-item-summary-refresh"
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        onClick={onRefresh}
                        aria-label="刷新当前任务"
                        title="刷新当前任务"
                    >
                        <RefreshCwIcon aria-hidden />
                    </Button>
                </div>
            </div>

            {authentication ? (
                <Alert variant={recovered ? "success" : "warning"}>
                    {recovered ? (
                        <CheckCircle2Icon aria-hidden />
                    ) : (
                        <ShieldAlertIcon aria-hidden />
                    )}
                    <AlertTitle>
                        {recovered
                            ? terminal
                                ? "鉴权已恢复"
                                : "鉴权已恢复，待确认完成"
                            : "接口授权异常，需要修复配置"}
                    </AlertTitle>
                    <AlertDescription>
                        {recovered
                            ? "当前配置的最新鉴权检查已通过，请核对恢复结果后完成处理。"
                            : "核对账号授权、凭据和签名配置，重新执行鉴权检查后返回刷新。"}
                    </AlertDescription>
                </Alert>
            ) : item.classification.errorClass === "result-unknown" ? (
                <Alert variant="warning">
                    <ShieldAlertIcon aria-hidden />
                    <AlertTitle>先查询原请求结果</AlertTitle>
                    <AlertDescription>
                        确认原下单、取消或退款的处理结果后，再决定后续操作。
                    </AlertDescription>
                </Alert>
            ) : (
                <div className="flex flex-wrap items-center gap-2">
                    <BusinessStatusBadge
                        context="detail"
                        label={item.classification.label}
                        tone={
                            item.classification.severity === "critical"
                                ? "destructive"
                                : "warning"
                        }
                    />
                    {item.compensationOpen ? (
                        <Badge variant="destructive">后续补救未完成</Badge>
                    ) : null}
                    {item.classification.errorClass ===
                    "parameter-or-mapping" ? (
                        <span className="text-body-compact text-muted-foreground">
                            请先修正关联的商品、客户或供应商资料，再重新核对。
                        </span>
                    ) : null}
                </div>
            )}

            {failure ? (
                <div className="space-y-2 border-l-2 border-border pl-4">
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>原始异常 · {failure.operation}</span>
                        {failure.failedAt ? (
                            <time dateTime={failure.failedAt}>
                                {formatDateTime(failure.failedAt, "default")}
                            </time>
                        ) : null}
                    </div>
                    <p className="break-words text-body-compact leading-relaxed">
                        {failureMessage ||
                            "原始失败摘要暂不可用，请打开接口设置查看检查与同步记录。"}
                    </p>
                    {error?.[1] || failure.jobNumber ? (
                        <details className="group text-xs text-muted-foreground">
                            <summary
                                id="integration-failure-technical-details"
                                className="inline-flex cursor-pointer list-none items-center gap-1 rounded-sm py-1 outline-none focus-visible:ring-2 focus-visible:ring-ring"
                            >
                                技术详情{" "}
                                <ChevronDownIcon
                                    className="size-3 transition-transform group-open:rotate-180"
                                    aria-hidden
                                />
                            </summary>
                            <div className="mt-1 space-y-1 break-all">
                                {error?.[1] ? (
                                    <div>错误代码：{error[1]}</div>
                                ) : null}
                                {failure.jobNumber ? (
                                    <div>检查编号：{failure.jobNumber}</div>
                                ) : null}
                            </div>
                        </details>
                    ) : null}
                </div>
            ) : null}

            {item.repairLinks.length > 0 ? (
                <div className="flex flex-wrap items-center gap-2">
                    {item.repairLinks.map((repairLink) => (
                        <Button
                            key={repairLink.href}
                            id={`integration-item-summary-repair-${toAutomationIdSegment(repairLink.href)}`}
                            type="button"
                            size="sm"
                            variant="outline"
                            render={<Link href={repairLink.href} />}
                        >
                            <ExternalLinkIcon
                                data-icon="inline-start"
                                aria-hidden
                            />
                            {repairLink.label}
                        </Button>
                    ))}
                    {failure?.operation === "商品目录同步" ? (
                        <span className="text-xs text-muted-foreground">
                            鉴权恢复后，仍需到商品库重新同步。
                        </span>
                    ) : null}
                </div>
            ) : null}

            {item.originalAction || item.message ? (
                <dl className="grid gap-x-6 gap-y-3 text-body-compact sm:grid-cols-2">
                    {item.originalAction ? (
                        <Fact
                            label="原动作"
                            value={item.originalAction.actionLabel}
                        />
                    ) : null}
                    {item.message ? (
                        <>
                            <Fact
                                label="同步方向"
                                value={item.message.directionLabel}
                            />
                            <Fact
                                label="事件摘要"
                                value={item.message.eventIdSummary}
                            />
                            <Fact
                                label="消息摘要"
                                value={item.message.maskedPayloadSummary}
                            />
                        </>
                    ) : null}
                </dl>
            ) : null}
            {item.difference ? (
                <>
                    <BusinessDiffPanel
                        className={
                            flat
                                ? "rounded-none border-x-0 border-border/60 [&_th:first-child]:pl-0 [&_td:first-child]:pl-0 [&>header]:px-0"
                                : undefined
                        }
                        title="双方数据核对"
                        caption="外部账单与系统记录对照"
                        fieldColumnLabel="对照项"
                        beforeColumnLabel={item.difference.leftLabel}
                        afterColumnLabel={item.difference.rightLabel}
                        noteColumnLabel="差异说明"
                        count={2}
                        changes={[
                            {
                                id: "side",
                                field: "对照摘要",
                                before: item.difference.leftSummary,
                                after: item.difference.rightSummary,
                                note: "双方数据只读对照",
                            },
                            {
                                id: "summary",
                                field: "差异摘要",
                                before: "—",
                                after: item.difference.differenceSummary,
                                note:
                                    DIFFERENCE_TYPE_LABEL[
                                        item.difference.differenceType.toUpperCase()
                                    ] ?? "差异说明未提供",
                            },
                        ]}
                    />
                    <p className="text-xs text-muted-foreground">
                        数据范围 {item.difference.boundary} · 更新时间{" "}
                        {formatDateTime(item.difference.watermark, "default")}
                    </p>
                </>
            ) : null}
        </section>
    )
}

function Fact({ label, value }: { label: string; value: React.ReactNode }) {
    return (
        <div className="min-w-0 space-y-1">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="break-words">{value}</dd>
        </div>
    )
}
