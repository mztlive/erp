"use client"

import { HistoryIcon, UserCogIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    Timeline,
    TimelineDescription,
    TimelineHeader,
    TimelineItem,
    TimelineMarker,
    TimelineTime,
    TimelineTitle,
} from "@/components/ui/timeline"
import {
    can,
    currentCollaborators,
    currentOwners,
    historicalAssignments,
} from "@/features/customers/pages/customer-detail-helpers"
import { periodLabel } from "@/features/customers/pages/customer-detail-records"
import type { CustomerCenterView } from "@/features/customers/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

export function CustomerDetailAuditTab({
    customer,
    refetch,
    onManageAssignments,
}: {
    customer: CustomerCenterView
    refetch: () => void
    onManageAssignments: () => void
}) {
    const owners = currentOwners(customer)
    const collaborators = currentCollaborators(customer)
    const historyAssignments = historicalAssignments(customer)
    const orderedAssignments = [
        ...owners,
        ...collaborators,
        ...historyAssignments,
    ]

    if (customer.partitions.audit === "error") {
        return (
            <DetailWorkspaceSection title="归属与记录" width="full">
                <BusinessFailureState
                    kind="system"
                    description="销售归属与修改记录加载失败"
                    action={
                        <Button
                            id="customers-detail-audit-retry"
                            type="button"
                            size="sm"
                            onClick={() => void refetch()}
                        >
                            重新加载
                        </Button>
                    }
                />
            </DetailWorkspaceSection>
        )
    }

    return (
        <>
            <DetailWorkspaceSection
                title="销售归属"
                badge={
                    owners.length > 0 || historyAssignments.length > 0 ? (
                        <div className="flex flex-wrap items-center gap-1.5">
                            {owners.length > 0 ? (
                                <Badge variant="neutral" className="num">
                                    {owners.length} 位负责销售
                                </Badge>
                            ) : null}
                            {historyAssignments.length > 0 ? (
                                <Badge variant="neutral" className="num">
                                    {historyAssignments.length} 条历史归属
                                </Badge>
                            ) : null}
                        </div>
                    ) : undefined
                }
                width="full"
                subtitle="多名销售可共同服务同一客户，各自负责的业务单据独立管理"
                actions={
                    can(customer, "MANAGE_ASSIGNMENTS") ? (
                        <Button
                            id="customers-detail-audit-manage-assignments"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.subtleButton}
                            onClick={onManageAssignments}
                        >
                            <UserCogIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            管理负责销售
                        </Button>
                    ) : undefined
                }
            >
                {orderedAssignments.length === 0 ? (
                    <BusinessEmptyState
                        kind="no-data"
                        title="暂无销售归属记录"
                        description="该客户尚未指派负责销售，点击右上角「管理负责销售」添加。"
                        className="rounded-lg border-0 bg-transparent px-0 py-4 shadow-none ring-0"
                    />
                ) : (
                    <div className={detailWorkspaceStyles.dataTableWrapper}>
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>销售人员</TableHead>
                                    <TableHead>角色</TableHead>
                                    <TableHead>归属状态</TableHead>
                                    <TableHead>调整原因</TableHead>
                                    <TableHead className="text-right">
                                        生效期间
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {orderedAssignments.map((assignment) => (
                                    <TableRow
                                        key={assignment.id}
                                        className={cn(
                                            !assignment.isCurrent &&
                                                "text-muted-foreground",
                                        )}
                                    >
                                        <TableCell>
                                            <span
                                                className={cn(
                                                    "font-medium",
                                                    assignment.isCurrent
                                                        ? "text-foreground"
                                                        : "text-foreground/75",
                                                )}
                                            >
                                                {assignment.userName}
                                            </span>
                                        </TableCell>
                                        <TableCell>
                                            <BusinessStatusBadge
                                                context="list"
                                                appearance={
                                                    assignment.isCurrent
                                                        ? "badge"
                                                        : "quiet"
                                                }
                                                label={
                                                    assignment.role === "OWNER"
                                                        ? "负责销售"
                                                        : "协作销售"
                                                }
                                                tone={
                                                    assignment.isCurrent &&
                                                    assignment.role === "OWNER"
                                                        ? "info"
                                                        : "neutral"
                                                }
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <StatusBadge
                                                appearance="quiet"
                                                tone={
                                                    assignment.isCurrent
                                                        ? "success"
                                                        : "neutral"
                                                }
                                                label={
                                                    assignment.isCurrent
                                                        ? "生效中"
                                                        : "已结束"
                                                }
                                            />
                                        </TableCell>
                                        <TableCell className="max-w-md whitespace-normal text-muted-foreground">
                                            {assignment.changeReason || "—"}
                                        </TableCell>
                                        <TableCell className="num text-right text-muted-foreground">
                                            {periodLabel(
                                                assignment.effectiveFrom,
                                                assignment.effectiveTo,
                                            )}
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                )}
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title="资料修改记录"
                badge={
                    customer.revisionTimeline.length > 0 ? (
                        <Badge variant="neutral" className="num">
                            共 {customer.revisionTimeline.length} 个版本
                        </Badge>
                    ) : undefined
                }
                width="full"
                subtitle="每次保存资料将生成新版本，历史单据沿用当时版本快照"
            >
                {customer.revisionTimeline.length === 0 ? (
                    <p className="py-1 text-xs text-muted-foreground">
                        暂无修改记录
                    </p>
                ) : (
                    <Timeline
                        aria-label="客户资料版本记录"
                        className="max-w-4xl"
                    >
                        {customer.revisionTimeline.map((revision) => (
                            <TimelineItem key={revision.id}>
                                <TimelineMarker>
                                    <HistoryIcon aria-hidden="true" />
                                </TimelineMarker>
                                <TimelineHeader>
                                    <TimelineTitle className="flex flex-wrap items-center gap-2">
                                        <span className="num">
                                            版本 v{revision.revisionNo}
                                        </span>
                                        {revision.isCurrent ? (
                                            <StatusBadge
                                                tone="info"
                                                label="当前版本"
                                            />
                                        ) : null}
                                    </TimelineTitle>
                                    <TimelineTime
                                        dateTime={revision.effectiveAt}
                                    >
                                        {formatDateTime(revision.effectiveAt)}
                                    </TimelineTime>
                                </TimelineHeader>
                                <TimelineDescription>
                                    <div className="text-foreground">
                                        {revision.reason || "资料修订"}
                                    </div>
                                    {revision.legalName ? (
                                        <div className="mt-1 text-xs text-muted-foreground">
                                            档案名称：{revision.legalName}
                                            {revision.shortName &&
                                            revision.shortName !==
                                                revision.legalName
                                                ? `（${revision.shortName}）`
                                                : ""}
                                        </div>
                                    ) : null}
                                </TimelineDescription>
                            </TimelineItem>
                        ))}
                    </Timeline>
                )}
            </DetailWorkspaceSection>
        </>
    )
}
