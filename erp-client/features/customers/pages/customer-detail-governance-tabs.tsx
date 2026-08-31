"use client"

import Link from "next/link"

import {
    AsyncSectionState,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    DocumentSection,
    DocumentSummary,
    surfaceInsetClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    CustomerAssignmentView,
    CustomerCenterView,
} from "@/features/customers/types"
import { can } from "@/features/customers/pages/customer-detail-helpers"

export function CustomerDetailQualityTab({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    const qualityHref = `/analytics/customer-quality?customerId=${encodeURIComponent(customer.customerId)}`

    return (
        <div className="space-y-4 pt-4">
            <DocumentSection
                title="经营摘要"
                description="数据由系统汇总；标签以系统返回为准。"
                action={
                    <Button
                        id="customers-detail-quality-open"
                        type="button"
                        size="sm"
                        variant="ghost"
                        render={<Link href={qualityHref} />}
                    >
                        打开经营质量
                    </Button>
                }
            >
                <AsyncSectionState
                    status={
                        customer.partitions.quality === "error"
                            ? "error"
                            : "success"
                    }
                    error="经营数据分区暂时不可用。已确认的客户主体与其它分区不受影响。"
                    errorKind="projection"
                    retryAction={
                        <Button
                            id="customers-detail-quality-retry"
                            type="button"
                            size="sm"
                            onClick={() => void refetch()}
                        >
                            重试经营分区
                        </Button>
                    }
                >
                    {customer.partitions.quality === "ok" &&
                    customer.qualitySummary ? (
                        <div className="space-y-3">
                            <DocumentSummary
                                columns="two"
                                items={[
                                    {
                                        id: "scale",
                                        label: "规模标签",
                                        value: customer.qualitySummary
                                            .scaleLabel,
                                    },
                                    {
                                        id: "profit",
                                        label: "利润贡献",
                                        value: customer.qualitySummary
                                            .profitContributionLabel,
                                    },
                                    {
                                        id: "risk",
                                        label: "回款风险",
                                        value: customer.qualitySummary
                                            .collectionRiskLabel,
                                    },
                                    {
                                        id: "lastBiz",
                                        label: "最近业务",
                                        value:
                                            customer.qualitySummary
                                                .lastBusinessAt ?? "—",
                                    },
                                ]}
                            />
                            <DataFreshness
                                updatedAt={customer.qualitySummary.projectionAt
                                    .slice(0, 16)
                                    .replace("T", " ")}
                                dateTime={customer.qualitySummary.projectionAt}
                                state={
                                    customer.qualitySummary.isStale
                                        ? "stale"
                                        : "fresh"
                                }
                                label="经营质量汇总于"
                            />
                        </div>
                    ) : customer.partitions.quality === "ok" ? (
                        <BusinessEmptyState
                            kind="no-data"
                            title="暂无经营摘要"
                            description="数据尚未生成。"
                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        />
                    ) : null}
                </AsyncSectionState>
            </DocumentSection>
        </div>
    )
}

export function CustomerDetailAuditTab({
    customer,
    refetch,
    onManageAssignments,
    onEndCollaboration,
}: {
    customer: CustomerCenterView
    refetch: () => void
    onManageAssignments: () => void
    onEndCollaboration: (target: CustomerAssignmentView) => void
}) {
    return (
        <div className="space-y-4 pt-4">
            <DocumentSection
                title="归属与审计"
                description="每位客户只有一位负责销售；协作销售显示有效期"
                action={
                    can(customer, "MANAGE_ASSIGNMENTS") ? (
                        <Button
                            id="customers-detail-audit-manage-assignments"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onManageAssignments}
                        >
                            调整归属
                        </Button>
                    ) : undefined
                }
            >
                {customer.partitions.audit === "error" ? (
                    <BusinessFailureState
                        kind="system"
                        description="归属审计分区失败。"
                        action={
                            <Button
                                id="customers-detail-audit-retry"
                                type="button"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : (
                    <div className="grid gap-4 lg:grid-cols-2">
                        <Card
                            size="sm"
                            className="shadow-none ring-1 ring-foreground/[0.04]"
                        >
                            <CardHeader className="border-b border-grid">
                                <CardTitle className="text-sm">
                                    当前责任关系
                                </CardTitle>
                            </CardHeader>
                            <CardContent className="space-y-2 text-sm">
                                {customer.assignments
                                    .filter((a) => a.isCurrent)
                                    .map((a) => (
                                        <div
                                            key={a.id}
                                            className={cn(
                                                surfaceInsetClassName,
                                                "flex flex-wrap items-center justify-between gap-2 px-3 py-2",
                                            )}
                                        >
                                            <div>
                                                <BusinessStatusBadge
                                                    context="list"
                                                    label={
                                                        a.role === "OWNER"
                                                            ? "负责销售"
                                                            : "协作销售"
                                                    }
                                                    tone={
                                                        a.role === "OWNER"
                                                            ? "info"
                                                            : "neutral"
                                                    }
                                                />
                                                <span className="ml-2 font-medium">
                                                    {a.userName}
                                                </span>
                                            </div>
                                            <div className="flex items-center gap-2">
                                                <span className="text-xs text-muted-foreground">
                                                    {a.effectiveFrom}
                                                    {a.effectiveTo
                                                        ? ` ~ ${a.effectiveTo}`
                                                        : " 起"}
                                                </span>
                                                {a.role === "COLLABORATOR" &&
                                                can(
                                                    customer,
                                                    "MANAGE_ASSIGNMENTS",
                                                ) ? (
                                                    <Button
                                                        id={`customers-detail-audit-${toAutomationIdSegment(a.id)}-end-collaboration`}
                                                        type="button"
                                                        size="xs"
                                                        variant="ghost"
                                                        onClick={() =>
                                                            onEndCollaboration(
                                                                a,
                                                            )
                                                        }
                                                    >
                                                        结束协作
                                                    </Button>
                                                ) : null}
                                            </div>
                                        </div>
                                    ))}
                            </CardContent>
                        </Card>
                        <Card
                            size="sm"
                            className="shadow-none ring-1 ring-foreground/[0.04]"
                        >
                            <CardHeader className="border-b border-grid">
                                <CardTitle className="text-sm">
                                    修订时间线
                                </CardTitle>
                                <CardDescription>
                                    新版本不覆盖历史合同/销售单记录
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-2 text-sm">
                                {customer.revisionTimeline.map((r) => (
                                    <div
                                        key={r.id}
                                        className={cn(
                                            surfaceInsetClassName,
                                            "px-3 py-2",
                                        )}
                                    >
                                        <div className="flex flex-wrap items-center gap-2">
                                            <span className="num font-medium">
                                                v{r.revisionNo}
                                            </span>
                                            {r.isCurrent ? (
                                                <Badge variant="secondary">
                                                    当前
                                                </Badge>
                                            ) : null}
                                            <span className="text-muted-foreground">
                                                {r.actor}
                                            </span>
                                        </div>
                                        <p className="mt-1 text-muted-foreground">
                                            {r.reason}
                                        </p>
                                        <p className="mt-0.5 text-xs text-muted-foreground">
                                            {r.effectiveAt}
                                        </p>
                                    </div>
                                ))}
                            </CardContent>
                        </Card>
                    </div>
                )}
            </DocumentSection>
        </div>
    )
}
