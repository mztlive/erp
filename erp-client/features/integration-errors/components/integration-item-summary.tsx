import type * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon, RefreshCwIcon, ShieldAlertIcon } from "lucide-react"

import {
    BusinessDiffPanel,
    BusinessStatusBadge,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { formatDateTime } from "@/lib/datetime"
import type { IntegrationResolutionItemView } from "../types"
import { DIFFERENCE_TYPE_LABEL, FUNDS_LABEL } from "../types"

function severityTone(
    severity: IntegrationResolutionItemView["classification"]["severity"],
): "destructive" | "warning" | "info" | "neutral" {
    if (severity === "critical") return "destructive"
    if (severity === "high") return "warning"
    if (severity === "medium") return "info"
    return "neutral"
}

export function IntegrationItemSummary({
    item,
    headingRef,
    onRefresh,
}: {
    item: IntegrationResolutionItemView
    headingRef: React.RefObject<HTMLHeadingElement | null>
    onRefresh: () => void
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle
                    ref={headingRef}
                    tabIndex={-1}
                    className="outline-none"
                >
                    {item.identity.number} · {item.businessObject.title}
                </CardTitle>
                <CardDescription>
                    {item.identity.itemType === "ERROR_TASK"
                        ? "错误任务"
                        : "对账差异"}
                    {item.workItem
                        ? " · 关联任务"
                        : " · 无关联任务（直接对账）"}
                </CardDescription>
                <div className="flex flex-wrap gap-2 pt-1">
                    <BusinessStatusBadge
                        context="detail"
                        label={item.classification.label}
                        tone={severityTone(item.classification.severity)}
                    />
                    <Badge variant="outline">
                        环境：{item.environmentLabel}
                    </Badge>
                    <Badge variant="outline">
                        严重度：{item.classification.severityLabel}
                    </Badge>
                    <Badge variant="outline">状态：{item.status.label}</Badge>
                    <Badge variant="outline">
                        {FUNDS_LABEL[item.fundsImpact]}
                    </Badge>
                    {item.compensationOpen ? (
                        <Badge variant="destructive">补偿未闭环</Badge>
                    ) : null}
                </div>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                {item.classification.errorClass === "result-unknown" ? (
                    <Alert variant="destructive">
                        <ShieldAlertIcon aria-hidden />
                        <AlertTitle>结果未知</AlertTitle>
                        <AlertDescription>
                            主动作仅为「查询原结果」。禁止直接重新提交下单/取消/退款。
                            系统按原任务号重发，不手动指定。
                        </AlertDescription>
                    </Alert>
                ) : null}

                {item.classification.errorClass ===
                    "authentication-or-signature" ||
                item.classification.errorClass === "parameter-or-mapping" ||
                item.classification.errorClass === "business-rejected" ? (
                    <Alert variant="warning">
                        <AlertTitle>
                            {item.classification.label} · 禁止无意义自动重试
                        </AlertTitle>
                        <AlertDescription>
                            页面不提供自动重试按钮；
                            {item.classification.errorClass ===
                            "authentication-or-signature"
                                ? "不展示密钥或完整签名材料。"
                                : item.classification.errorClass ===
                                    "parameter-or-mapping"
                                  ? "请先到供应商供给或商城同步修复主数据引用。"
                                  : "请进入供应商订单售后/补偿路径。"}
                        </AlertDescription>
                    </Alert>
                ) : null}

                <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                    <Fact label="滞留" value={item.ageLabel} />
                    <Fact
                        label="责任"
                        value={item.ownerUser ?? item.ownerRole}
                    />
                    <Fact
                        label="方向"
                        value={item.message?.directionLabel ?? "—"}
                    />
                    {item.originalAction ? (
                        <>
                            <Fact
                                label="原动作"
                                value={item.originalAction.actionLabel}
                            />
                            <Fact
                                label="原任务号摘要"
                                value={
                                    item.originalAction
                                        .originalActionIdempotencyKeySummary
                                }
                                mono
                            />
                        </>
                    ) : null}
                    {item.message ? (
                        <>
                            <Fact
                                label="事件摘要"
                                value={item.message.eventIdSummary}
                                mono
                            />
                            <Fact
                                label="消息摘要"
                                value={item.message.maskedPayloadSummary}
                            />
                        </>
                    ) : null}
                </div>

                {item.difference ? (
                    <>
                        <BusinessDiffPanel
                            title="对账左右证据"
                            caption="左右侧金额与行数对照"
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
                                    note: "两侧对照，只读证据",
                                },
                                {
                                    id: "summary",
                                    field: "差异摘要",
                                    before: "—",
                                    after: item.difference.differenceSummary,
                                    note:
                                        DIFFERENCE_TYPE_LABEL[
                                            item.difference.differenceType
                                        ] ?? item.difference.differenceType,
                                },
                            ]}
                        />
                        <p className="text-xs text-muted-foreground">
                            数据范围 {item.difference.boundary} · 更新时间{" "}
                            {formatDateTime(
                                item.difference.watermark,
                                "default",
                            )}
                        </p>
                    </>
                ) : null}

                {item.repairLinks.length > 0 ? (
                    <div className="flex flex-wrap gap-2">
                        {item.repairLinks.map((repairLink) => (
                            <Button
                                key={repairLink.href}
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
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onRefresh}
                        >
                            <RefreshCwIcon
                                data-icon="inline-start"
                                aria-hidden
                            />
                            刷新当前任务
                        </Button>
                    </div>
                ) : null}
            </CardContent>
        </Card>
    )
}

function Fact({
    label,
    value,
    mono,
}: {
    label: string
    value: React.ReactNode
    mono?: boolean
}) {
    return (
        <div className="space-y-0.5">
            <div className="text-xs text-muted-foreground">{label}</div>
            <div className={mono ? "num font-mono text-sm" : "text-sm"}>
                {value}
            </div>
        </div>
    )
}
