"use client"

import { useEffect, useState, type ReactNode } from "react"
import {
    AlertTriangleIcon,
    ArrowUpRightIcon,
    RefreshCwIcon,
    TriangleAlertIcon,
} from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { StatusBadge } from "@/components/ui/status-badge"
import { usePortalReviewApplication } from "@/features/supplier-portal-admin/hooks"
import {
    PORTAL_REVIEW_STORAGE_EVENT,
    portalReviewStorageKey,
    readPortalReviewAttempt,
    readPortalReviewDraft,
} from "@/features/supplier-portal-admin/lib/review-draft"
import {
    portalReviewMaterialRefs,
    portalReviewOverview,
} from "@/features/supplier-portal-admin/lib/review-overview"
import {
    portalApplicationStatus,
    timeLabel,
} from "@/features/supplier-portal/lib/presentation"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskResponsibilityActions } from "./workspace-task-responsibility-actions"

export function WorkspaceSupplierPortalInspector({
    item,
    onOpenFullPage,
    onCompleted,
    presentation,
}: {
    item: WorkspaceWorkItem
    onOpenFullPage: () => void
    onCompleted?: (completedId: string) => void
    presentation: "pane" | "sheet"
}) {
    const { access, canRead, query, application } = usePortalReviewApplication(
        item.businessObjectId,
    )
    const attachmentCount = portalReviewMaterialRefs(application).length
    const overview = application ? portalReviewOverview(application) : undefined
    const prefix = `workspace-supplier-portal-inspector-${toAutomationIdSegment(item.workItemId)}`
    const pad = presentation === "sheet" ? "px-sheet-inline" : "px-5"
    const actor = access.profile.data?.userid
    const storageKey =
        actor && application && access.can("supplier_portal_request:review")
            ? portalReviewStorageKey(actor, application.id, item.workItemId)
            : undefined
    const version = application?.version
    const blocked = isBlockedWorkItem(item)
    const overdue = item.status === "OPEN" && item.dueBucket === "overdue"
    const canManage =
        presentation === "pane" &&
        access.can("work_item:manage") &&
        item.status === "OPEN" &&
        item.allowedActions.some(
            (action) => action === "REASSIGN" || action === "CLOSE",
        )
    const [localState, setLocalState] = useState<"attempt" | "draft" | null>(
        null,
    )

    useEffect(() => {
        if (!storageKey || version === undefined) {
            setLocalState(null)
            return
        }
        const sync = () => {
            if (readPortalReviewAttempt(storageKey, item.workItemId)) {
                setLocalState("attempt")
            } else {
                setLocalState(
                    readPortalReviewDraft(storageKey, version) ? "draft" : null,
                )
            }
        }
        const onStorage = (event: StorageEvent) => {
            if (
                !event.key ||
                event.key === storageKey ||
                event.key === `${storageKey}:attempt`
            )
                sync()
        }
        const onLocalChange = (event: Event) => {
            if (
                event instanceof CustomEvent &&
                (event.detail === storageKey ||
                    event.detail === `${storageKey}:attempt`)
            ) {
                sync()
            }
        }
        sync()
        window.addEventListener("storage", onStorage)
        window.addEventListener(PORTAL_REVIEW_STORAGE_EVENT, onLocalChange)
        window.addEventListener("focus", sync)
        return () => {
            window.removeEventListener("storage", onStorage)
            window.removeEventListener(
                PORTAL_REVIEW_STORAGE_EVENT,
                onLocalChange,
            )
            window.removeEventListener("focus", sync)
        }
    }, [item.workItemId, storageKey, version])

    const qualifiedTotal = overview?.qualificationChecks.length ?? 0
    const qualifiedCount = overview?.qualifiedCount ?? 0
    const qualifiedComplete =
        qualifiedTotal > 0 && qualifiedCount >= qualifiedTotal
    const deltaTone =
        overview?.priceDelta?.direction === "up"
            ? "warning"
            : overview?.priceDelta?.direction === "down"
              ? "success"
              : "default"

    return (
        <div
            data-slot="workspace-supplier-portal-inspector"
            className="@container/supplier-portal-inspector flex min-h-0 w-full min-w-0 flex-1 flex-col overflow-hidden bg-card"
        >
            <div
                data-slot="workspace-task-inspector-controls"
                className={cn(
                    "flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken/60 py-1.5 text-xs",
                    pad,
                )}
            >
                <div className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-muted-foreground">
                        {localState === "attempt"
                            ? "供应商申请快览（上次提交待核对）"
                            : localState === "draft"
                              ? "供应商申请快览（含未提交草稿）"
                              : "供应商申请快览"}
                    </span>
                    {localState === "attempt" ? (
                        <StatusBadge
                            label="待核对"
                            tone="warning"
                            appearance="quiet"
                        />
                    ) : localState === "draft" ? (
                        <StatusBadge
                            label="已暂存"
                            tone="info"
                            appearance="quiet"
                        />
                    ) : null}
                    {overdue ? (
                        <StatusBadge label="已超期" tone="destructive" />
                    ) : null}
                </div>
                {canRead ? (
                    <Button
                        id={`${prefix}-refresh`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className="h-control-xs shrink-0 px-2 text-xs text-muted-foreground hover:text-foreground"
                        disabled={query.isFetching}
                        onClick={() => void query.refetch()}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                            className={cn(query.isFetching && "animate-spin")}
                        />
                        刷新资料
                    </Button>
                ) : null}
            </div>
            <div
                className={cn(
                    "min-h-0 min-w-0 flex-1 space-y-4 overflow-y-auto overflow-x-hidden py-4",
                    pad,
                )}
            >
                {access.profile.isPending || (canRead && query.isPending) ? (
                    <div role="status" className="space-y-4">
                        <span className="sr-only">正在读取供应商申请…</span>
                        <div className="grid grid-cols-2 gap-2 @[520px]/supplier-portal-inspector:grid-cols-4">
                            {["status", "proposal", "checks", "files"].map(
                                (key) => (
                                    <Skeleton
                                        key={key}
                                        className="h-14 w-full"
                                    />
                                ),
                            )}
                        </div>
                    </div>
                ) : access.profile.isError ||
                  (!application && query.isError) ? (
                    <BusinessFailureState
                        idPrefix={`${prefix}-failure`}
                        title={
                            access.profile.isError
                                ? "账号权限信息读取失败"
                                : "申请资料读取失败"
                        }
                        error={access.profile.error ?? query.error}
                        onRetry={() =>
                            void (access.profile.isError
                                ? access.profile.refetch()
                                : query.refetch())
                        }
                    />
                ) : !canRead ? (
                    <BusinessFailureState
                        kind="permission"
                        title="无权查看供应商申请"
                    />
                ) : application && overview ? (
                    <>
                        {blocked && item.actionBlockers.length > 0 ? (
                            <div
                                role="alert"
                                className="flex items-start gap-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-xs text-warning-soft-foreground"
                            >
                                <AlertTriangleIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0"
                                />
                                <p className="min-w-0 flex-1 wrap-anywhere">
                                    {item.actionBlockers
                                        .map((blocker) => blocker.message)
                                        .join("；")}
                                </p>
                            </div>
                        ) : null}
                        {localState === "attempt" ? (
                            <div
                                role="status"
                                className="flex items-start gap-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-xs text-warning-soft-foreground"
                            >
                                <TriangleAlertIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0"
                                />
                                <p className="min-w-0 flex-1 wrap-anywhere">
                                    上次提交结果待核对，请点击右下角进入完整作业面核对提交结果。
                                </p>
                            </div>
                        ) : null}
                        {overview.outstanding.length > 0 ? (
                            <div
                                role="status"
                                className="flex items-start gap-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft/75 px-3 py-2 text-xs text-warning-soft-foreground"
                            >
                                <TriangleAlertIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0"
                                />
                                <p className="min-w-0 flex-1 wrap-anywhere">
                                    尚有{" "}
                                    <span className="num font-semibold">
                                        {overview.outstanding.length}
                                    </span>{" "}
                                    项提报资料待核对：
                                    {overview.outstanding
                                        .map((check) => check.label)
                                        .join("、")}
                                </p>
                            </div>
                        ) : null}
                        <dl
                            aria-label="供应商申请快览指标"
                            className="grid grid-cols-2 gap-2 @[520px]/supplier-portal-inspector:grid-cols-4"
                        >
                            <InspectorKpi label="申请状态">
                                <StatusBadge
                                    {...portalApplicationStatus(
                                        application.status,
                                    )}
                                />
                            </InspectorKpi>
                            <InspectorKpi
                                label={
                                    overview.price !== null
                                        ? overview.priceLabel
                                        : application.kind === "new_product"
                                          ? "提报规格"
                                          : "申请类型"
                                }
                            >
                                {overview.price !== null ? (
                                    <MoneyValue
                                        value={overview.price}
                                        display="exact"
                                        className="max-w-full [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal"
                                        inheritColor
                                    />
                                ) : application.kind === "new_product" ? (
                                    <span className="num">
                                        {overview.skuCount} 项
                                    </span>
                                ) : (
                                    overview.kind
                                )}
                            </InspectorKpi>
                            <InspectorKpi label="提报资料">
                                <span className="num">
                                    {qualifiedCount}/{qualifiedTotal} 项
                                </span>
                                {qualifiedTotal > 0 ? (
                                    <div
                                        aria-hidden="true"
                                        className="mt-1.5 flex h-1.5 w-full overflow-hidden rounded-full bg-muted"
                                    >
                                        {qualifiedCount > 0 ? (
                                            <span
                                                className={
                                                    qualifiedComplete
                                                        ? "bg-success"
                                                        : "bg-warning"
                                                }
                                                style={{
                                                    flexGrow: qualifiedCount,
                                                }}
                                            />
                                        ) : null}
                                        {qualifiedTotal > qualifiedCount ? (
                                            <span
                                                className="bg-transparent"
                                                style={{
                                                    flexGrow:
                                                        qualifiedTotal -
                                                        qualifiedCount,
                                                }}
                                            />
                                        ) : null}
                                    </div>
                                ) : null}
                            </InspectorKpi>
                            <InspectorKpi label="附件">
                                <span className="num">
                                    {attachmentCount} 份
                                </span>
                            </InspectorKpi>
                        </dl>
                        <WorkspaceFactGrid columns={2}>
                            {application.application_no ? (
                                <WorkspaceFactItem
                                    label="申请单号"
                                    value={application.application_no}
                                    numeric
                                    selectable
                                />
                            ) : null}
                            {overview.supplier || item.counterpartyName ? (
                                <WorkspaceFactItem
                                    label="供应商"
                                    value={
                                        overview.supplier ||
                                        item.counterpartyName
                                    }
                                />
                            ) : null}
                            {overview.kind ? (
                                <WorkspaceFactItem
                                    label="申请类型"
                                    value={overview.kind}
                                />
                            ) : null}
                            {overview.title || overview.orderingCode ? (
                                <WorkspaceFactItem
                                    label="提报商品或供货内容"
                                    value={
                                        overview.title || overview.orderingCode
                                    }
                                />
                            ) : null}
                            {overview.submittedAt != null ? (
                                <WorkspaceFactItem
                                    label="提交时间"
                                    value={timeLabel(overview.submittedAt)}
                                    numeric
                                />
                            ) : null}
                            {overview.impact ? (
                                <WorkspaceFactItem
                                    label="生效影响"
                                    value={overview.impact}
                                    numeric={Boolean(overview.priceDelta)}
                                    tone={deltaTone}
                                />
                            ) : null}
                            {item.dueAt && item.dueAtLabel ? (
                                <WorkspaceFactItem
                                    label="任务截止"
                                    value={item.dueAtLabel}
                                    numeric
                                />
                            ) : null}
                            {overview.reason ? (
                                <WorkspaceFactItem
                                    label="提交说明"
                                    value={overview.reason}
                                    span="full"
                                />
                            ) : null}
                        </WorkspaceFactGrid>
                        <WorkspaceFactGroup title="资料核对" divider>
                            <WorkspaceFactGrid columns={2}>
                                {overview.qualificationChecks.map(
                                    ({ label, value }) => (
                                        <WorkspaceFactItem
                                            key={label}
                                            label={label}
                                            value={value || "待补充"}
                                            tone={
                                                !value ? "warning" : "default"
                                            }
                                        />
                                    ),
                                )}
                            </WorkspaceFactGrid>
                        </WorkspaceFactGroup>
                    </>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        未找到该供应商申请。
                    </p>
                )}
            </div>
            {canRead || canManage ? (
                <footer
                    data-slot="workspace-inspector-footer"
                    className={cn(
                        "sticky bottom-0 flex min-h-12 min-w-0 shrink-0 flex-wrap items-center justify-end gap-2 border-t border-border/60 bg-card py-2 shadow-footer",
                        pad,
                    )}
                >
                    {canManage ? (
                        <div className="mr-auto flex min-w-0 flex-wrap gap-2">
                            <WorkspaceTaskResponsibilityActions
                                item={item}
                                onCompleted={onCompleted}
                            />
                        </div>
                    ) : null}
                    {canRead ? (
                        <Button
                            id={`${prefix}-open-full`}
                            type="button"
                            size="sm"
                            variant="default"
                            onClick={onOpenFullPage}
                        >
                            进入完整作业面调整
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    ) : null}
                </footer>
            ) : null}
        </div>
    )
}

function InspectorKpi({
    label,
    children,
}: {
    label: string
    children: ReactNode
}) {
    return (
        <div className="min-w-0 rounded-lg bg-surface-sunken px-3 py-2.5">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="mt-1 min-w-0 wrap-anywhere text-body-compact font-semibold">
                {children}
            </dd>
        </div>
    )
}
