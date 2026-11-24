"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { FileTextIcon, PanelRightCloseIcon, XIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceMeta,
    PaperDocumentViewport,
    useWorkspaceInspectorMode,
    useWorkspaceTaskPage,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { AcceptanceWorkspace } from "@/features/sales-orders/components/acceptance-workspace"
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayBusinessText } from "@/lib/display-name"
import { hasPermission } from "@/lib/permissions"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"

import { workspaceHomeKeys } from "../hooks/queries"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import {
    workspaceAcceptanceDescriptor,
    workspaceAcceptanceTaskIdentity,
} from "../lib/workspace-acceptance"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

export function WorkspaceAcceptanceTask(props: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return <AcceptanceTaskPane key={props.item.workItemId} {...props} />
}

function AcceptanceTaskPane({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const queryClient = useQueryClient()
    const descriptor = workspaceAcceptanceDescriptor(item)
    const executionAuthorized = item.allowedActions.includes("PROCESS")
    const idPrefix = `workspace-acceptance-${toAutomationIdSegment(item.workItemId)}`
    const profile = useAccountProfileQuery()
    const salesOrderId = descriptor?.salesOrderId
    const canReadSalesOrder =
        Boolean(salesOrderId) &&
        profile.isSuccess &&
        hasPermission(profile.data.permissions, "sales_order:detail")
    const orderQuery = useSalesOrderDetailQuery(
        canReadSalesOrder ? (salesOrderId ?? "") : "",
    )
    const order = canReadSalesOrder ? orderQuery.data : undefined
    const title = "客户验收登记"
    const salesOrderNo = displayBusinessText(
        order?.documentNumber || stripDocumentNumberPrefix(item.stableNumber),
        salesOrderId,
        title,
    )
    const customer = (order?.customerName || item.counterpartyName)?.trim()
    const owner = item.ownerUserLabel?.trim()
    const [railOpen, setRailOpen] = React.useState(false)
    const [narrowOpen, setNarrowOpen] = React.useState(false)
    const [inlineReference, setInlineReference] = React.useState(false)
    const [wide, setWide] = React.useState(false)
    const surfaceRef = React.useRef<HTMLDivElement>(null)
    const inspector = useWorkspaceInspectorMode()
    const fullPage = useWorkspaceTaskPage()
    const useRail = wide && !inspector

    React.useEffect(() => {
        const surface = surfaceRef.current
        if (!surface) return
        const target = surface.closest<HTMLElement>("#main-content") ?? surface
        const sync = () => setWide(target.clientWidth >= 960)
        sync()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(sync)
        observer.observe(target)
        return () => observer.disconnect()
    }, [])

    React.useEffect(() => {
        if (inspector || !canReadSalesOrder) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action !== "toggle-rail") return
            if (useRail) setRailOpen((open) => !open)
            else setNarrowOpen((open) => !open)
        })
    }, [canReadSalesOrder, inspector, useRail])

    const closeReference = () => {
        setRailOpen(false)
        setNarrowOpen(false)
    }

    const referenceActive = useRail ? railOpen : narrowOpen

    const reference =
        referenceActive && canReadSalesOrder ? (
            <div className="flex h-full min-h-0 min-w-0 flex-col bg-card">
                <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2.5 pr-1">
                    <div className="min-w-0">
                        <h2 className="truncate text-body-compact font-semibold text-foreground">
                            来源销售单对照
                        </h2>
                        {salesOrderNo ? (
                            <p className="num truncate text-xs text-muted-foreground select-all">
                                {salesOrderNo}
                            </p>
                        ) : null}
                    </div>
                    <Button
                        id={`${idPrefix}-reference-close`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label="收起来源销售单对照栏"
                        onClick={closeReference}
                    >
                        <XIcon aria-hidden="true" />
                    </Button>
                </div>
                <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-3">
                    {orderQuery.isPending ? (
                        <div className="flex min-h-48 items-center justify-center py-10">
                            <p className="flex items-center gap-2 text-xs text-muted-foreground">
                                <Spinner aria-hidden="true" />
                                正在读取销售单…
                            </p>
                        </div>
                    ) : orderQuery.isError ? (
                        <BusinessFailureState
                            title="销售单读取失败"
                            error={orderQuery.error}
                            action={
                                <Button
                                    id={`${idPrefix}-reference-retry`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={() => void orderQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : order ? (
                        <PaperDocumentViewport
                            id={`${idPrefix}-reference-paper`}
                            fitMode="width"
                            fitKey={salesOrderId}
                            role="region"
                            aria-label="销售单纸面"
                            tabIndex={0}
                            className="bg-transparent p-0"
                        >
                            <SalesOrderPaperDocument order={order} />
                        </PaperDocumentViewport>
                    ) : (
                        <p className="py-10 text-center text-xs text-muted-foreground">
                            未找到这张销售单，可能已删除或当前角色无权查看。
                        </p>
                    )}
                </div>
            </div>
        ) : null

    const renderPane = (body: React.ReactNode) => (
        <WorkspaceProcessingPane
            archetype="action"
            className="@container/document"
            idPrefix={idPrefix}
            contextRail={
                useRail && canReadSalesOrder
                    ? {
                          collapsible: true,
                          resizable: true,
                          allowOnFullWidth: true,
                          defaultWidthPercent: 38,
                          minWidthPercent: 26,
                          maxWidthPercent: 52,
                          storageKey: "erp.workspace.acceptanceRailWidth",
                          collapsed: !railOpen,
                          onCollapsedChange: (collapsed) =>
                              setRailOpen(!collapsed),
                          label: "对照来源销售单",
                          content: reference,
                      }
                    : undefined
            }
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title={title}
                    compact={!fullPage}
                    subtitle={
                        <DetailWorkspaceMeta
                            items={[
                                customer &&
                                customer !== title &&
                                customer !== salesOrderNo
                                    ? customer
                                    : null,
                                salesOrderNo ? (
                                    <span
                                        key="number"
                                        className="num select-all"
                                        data-workspace-document-number={
                                            salesOrderNo
                                        }
                                    >
                                        {salesOrderNo}
                                    </span>
                                ) : null,
                                owner ? `负责人 ${owner}` : null,
                                item.dueAt && item.dueAtLabel ? (
                                    <span key="deadline" className="num">
                                        截止 {item.dueAtLabel}
                                    </span>
                                ) : null,
                            ]}
                        />
                    }
                >
                    {canReadSalesOrder ? (
                        <Button
                            id={`${idPrefix}-reference-toggle`}
                            type="button"
                            variant={
                                (useRail || inlineReference) && referenceActive
                                    ? "secondary"
                                    : "ghost"
                            }
                            size="sm"
                            aria-pressed={
                                useRail || inlineReference
                                    ? referenceActive
                                    : undefined
                            }
                            aria-haspopup={
                                !useRail && !inlineReference
                                    ? "dialog"
                                    : undefined
                            }
                            onClick={(event) => {
                                if (useRail) {
                                    setRailOpen((open) => !open)
                                    return
                                }
                                setInlineReference(
                                    Boolean(
                                        event.currentTarget.closest(
                                            '[data-business-component="task-action-sheet"]',
                                        ),
                                    ),
                                )
                                setNarrowOpen((open) => !open)
                            }}
                        >
                            {(useRail || inlineReference) && referenceActive ? (
                                <PanelRightCloseIcon
                                    aria-hidden="true"
                                    data-icon="inline-start"
                                />
                            ) : (
                                <FileTextIcon
                                    aria-hidden="true"
                                    data-icon="inline-start"
                                />
                            )}
                            {(useRail || inlineReference) && referenceActive
                                ? "收起销售单"
                                : "对照销售单"}
                        </Button>
                    ) : null}
                </WorkspaceTaskIdentityHeader>
            }
            aria-label="当前客户验收任务"
        >
            {!useRail && inlineReference && narrowOpen ? reference : null}
            {body}
            {canReadSalesOrder && !useRail && !inlineReference ? (
                <SalesOrderPaperPreviewDialog
                    salesOrderId={salesOrderId ?? null}
                    title={salesOrderNo}
                    open={narrowOpen}
                    onOpenChange={setNarrowOpen}
                />
            ) : null}
        </WorkspaceProcessingPane>
    )

    return (
        <div ref={surfaceRef} className="flex min-h-0 min-w-0 flex-1 flex-col">
            {descriptor ? (
                <AcceptanceWorkspace
                    salesOrderId={descriptor.salesOrderId}
                    idPrefix={`${idPrefix}-registration`}
                    ownerName={item.ownerUserLabel}
                    workItem={workspaceAcceptanceTaskIdentity(item)}
                    persistRegisterInUrl={false}
                    inlineRegister
                    taskBlocker={
                        executionAuthorized
                            ? undefined
                            : (item.actionBlockers[0]?.message ??
                              "当前账号无法办理此验收任务。")
                    }
                    referenceOrder={order}
                    renderTaskPane={renderPane}
                    onPosted={(payload) => {
                        void queryClient.invalidateQueries({
                            queryKey: workspaceHomeKeys.all,
                        })
                        if (payload.remainingEligibleCount > 0) return
                        toast.add({
                            title: "客户验收已登记",
                            description: `${payload.acceptanceNo} · 本单待验已清零`,
                            type: "success",
                            timeout: 4000,
                        })
                        onTaskCompleted?.(item.workItemId)
                    }}
                />
            ) : (
                renderPane(
                    <div
                        role="alert"
                        className="my-3 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft px-3 py-2 text-xs text-destructive-soft-foreground"
                    >
                        待办与关联销售单不匹配。请刷新任务后重试。
                    </div>,
                )
            )}
        </div>
    )
}
