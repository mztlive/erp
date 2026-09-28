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
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { CustomerCenterView } from "@/features/customers/types"
import { can } from "@/features/customers/pages/customer-detail-helpers"
import {
    DetailRecordColumn,
    DetailRecordColumns,
    DetailRecordRow,
    detailSectionClassName,
    embeddedSummaryClassName,
    periodLabel,
} from "@/features/customers/pages/customer-detail-records"

export function CustomerDetailQualityTab({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    const qualityHref = `/analytics/customer-quality?customerId=${encodeURIComponent(customer.customerId)}`

    return (
        <DocumentSection
            className={detailSectionClassName}
            title="经营摘要"
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
                            className={embeddedSummaryClassName}
                            columns="four"
                            items={[
                                {
                                    id: "scale",
                                    label: "规模标签",
                                    value: customer.qualitySummary.scaleLabel,
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
    )
}

export function CustomerDetailAuditTab({
    customer,
    refetch,
    onManageAssignments,
}: {
    customer: CustomerCenterView
    refetch: () => void
    onManageAssignments: () => void
}) {
    const owners = customer.assignments.filter(
        (assignment) => assignment.isCurrent && assignment.role === "OWNER",
    )

    return (
        <DocumentSection
            className={detailSectionClassName}
            title="归属与审计"
            description="每位客户只有一位负责销售"
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
                <DetailRecordColumns>
                    <DetailRecordColumn label="当前责任关系">
                        {owners.length === 0 ? (
                            <p className="py-2 text-sm text-muted-foreground">
                                暂无负责销售
                            </p>
                        ) : (
                            owners.map((assignment) => (
                                <DetailRecordRow key={assignment.id}>
                                    <BusinessStatusBadge
                                        context="list"
                                        label="负责销售"
                                        tone="info"
                                    />
                                    <span className="font-medium">
                                        {assignment.userName}
                                    </span>
                                    <span className="ml-auto text-muted-foreground">
                                        {periodLabel(
                                            assignment.effectiveFrom,
                                            assignment.effectiveTo,
                                        )}
                                    </span>
                                </DetailRecordRow>
                            ))
                        )}
                    </DetailRecordColumn>
                    <DetailRecordColumn label="修订时间线">
                        {customer.revisionTimeline.length === 0 ? (
                            <p className="py-2 text-sm text-muted-foreground">
                                暂无修订记录
                            </p>
                        ) : (
                            customer.revisionTimeline.map((revision) => (
                                <DetailRecordRow key={revision.id}>
                                    <span className="num font-medium">
                                        v{revision.revisionNo}
                                    </span>
                                    {revision.isCurrent ? (
                                        <Badge variant="secondary">当前</Badge>
                                    ) : null}
                                    <span className="text-muted-foreground">
                                        {revision.actor}
                                    </span>
                                    <span className="min-w-0 text-muted-foreground">
                                        {revision.reason}
                                    </span>
                                    <span className="ml-auto text-xs text-muted-foreground">
                                        {revision.effectiveAt}
                                    </span>
                                </DetailRecordRow>
                            ))
                        )}
                    </DetailRecordColumn>
                </DetailRecordColumns>
            )}
        </DocumentSection>
    )
}
