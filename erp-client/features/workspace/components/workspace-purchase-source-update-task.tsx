"use client"

import { useEffect, useRef, useState } from "react"
import Link from "next/link"
import { usePathname, useSearchParams } from "next/navigation"
import {
    ArrowUpRightIcon,
    FileTextIcon,
    PanelRightCloseIcon,
    XIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceMeta,
    PaperDocumentViewport,
    WorkspaceProcessingPane,
} from "@/components/business"
import {
    useWorkspaceInspectorMode,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import { Button, buttonVariants } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { PurchaseOrderPaperDocument } from "@/features/purchase-orders/components/purchase-order-paper-document"
import { PurchaseSourceUpdateWorkspace } from "@/features/purchase-orders/components/purchase-source-update-workspace"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { TASK_TYPE_CANONICAL_LABELS } from "@/lib/task-status-dictionary"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentPaperDialog } from "./workspace-document-paper-dialog"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** 采购依据更新只在服务端确认任务完成后推进队列。 */
export function WorkspacePurchaseSourceUpdateTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const idPrefix = `workspace-source-update-${toAutomationIdSegment(item.workItemId)}`
    const [railOpen, setRailOpen] = useState(false)
    const [previewOpen, setPreviewOpen] = useState(false)
    const [wide, setWide] = useState(false)
    const rootRef = useRef<HTMLDivElement>(null)
    const toggleRef = useRef<HTMLButtonElement>(null)
    const inspectorMode = useWorkspaceInspectorMode()
    const fullPage = useWorkspaceTaskPage()
    const profileQuery = useAccountProfileQuery()
    const canReadOrder = hasPermission(
        profileQuery.data?.permissions,
        "purchase_order:detail",
    )

    useEffect(() => {
        if (inspectorMode || !rootRef.current) return
        const host =
            rootRef.current.closest<HTMLElement>("#main-content") ??
            rootRef.current
        const sync = () => {
            const nextWide = host.clientWidth >= 960
            setWide(nextWide)
            if (!nextWide) setRailOpen(false)
            else setPreviewOpen(false)
        }
        sync()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(sync)
        observer.observe(host)
        return () => observer.disconnect()
    }, [inspectorMode])

    const useRail = canReadOrder && wide && !inspectorMode
    const railExpanded = useRail && railOpen && !previewOpen
    useEffect(() => {
        if (inspectorMode || !canReadOrder) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action !== "toggle-rail") return
            if (useRail) setRailOpen((open) => !open)
            else setPreviewOpen((open) => !open)
        })
    }, [canReadOrder, inspectorMode, useRail])

    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = searchParams.size
        ? `${pathname}?${searchParams}`
        : pathname
    const purchaseOrderId = item.businessObjectId
    const purchaseHref = `/procurement/orders/${encodeURIComponent(purchaseOrderId)}?${new URLSearchParams({ from: "workspace", returnTo })}`
    const orderQuery = usePurchaseOrderCenterQuery(
        canReadOrder ? purchaseOrderId : "",
    )
    const { data: order } = orderQuery
    const taskQuery = useWorkItemDetailQuery(item.workItemId)
    const notified = useRef<string | null>(null)
    useEffect(() => {
        if (
            taskQuery.isError ||
            taskQuery.isFetching ||
            taskQuery.data?.status !== "COMPLETED" ||
            notified.current === item.workItemId
        )
            return
        notified.current = item.workItemId
        onTaskCompleted?.(item.workItemId)
    }, [
        taskQuery.data?.status,
        taskQuery.isError,
        taskQuery.isFetching,
        item.workItemId,
        onTaskCompleted,
    ])

    const title =
        item.workItemTypeLabel ||
        TASK_TYPE_CANONICAL_LABELS.PURCHASE_SOURCE_UPDATE.label
    const stableNumber = stripDocumentNumberPrefix(item.stableNumber)
    const purchaseNo =
        order?.identity.purchaseNo ||
        (stableNumber.trim() === title.trim() ? "" : stableNumber)
    const supplier = order?.header.supplierSnapshot || item.counterpartyName
    const owner = order?.header.ownerName || item.ownerUserLabel
    const closeRail = () => {
        setRailOpen(false)
        toggleRef.current?.focus()
    }

    return (
        <div ref={rootRef} className="h-full min-h-0 min-w-0 bg-card">
            <PurchaseSourceUpdateWorkspace
                idPrefix={idPrefix}
                purchaseOrderId={purchaseOrderId}
                returnTo={returnTo}
                onRefreshTask={() => taskQuery.refetch()}
                renderSurface={(body) => (
                    <WorkspaceProcessingPane
                        archetype="action"
                        className="@container/document"
                        idPrefix={idPrefix}
                        contextRail={
                            useRail
                                ? {
                                      collapsible: true,
                                      resizable: true,
                                      allowOnFullWidth: true,
                                      defaultWidthPercent: 38,
                                      minWidthPercent: 26,
                                      maxWidthPercent: 52,
                                      storageKey:
                                          "workspace-source-update-rail-width",
                                      collapsed: !railExpanded,
                                      onCollapsedChange: (collapsed) =>
                                          setRailOpen(!collapsed),
                                      label: "对照采购单",
                                      content: (
                                          <div className="flex h-full min-h-0 min-w-0 flex-col bg-card">
                                              <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2.5 pr-1">
                                                  <div className="min-w-0">
                                                      <h2 className="truncate text-body-compact font-semibold">
                                                          对照采购单
                                                      </h2>
                                                      {purchaseNo ? (
                                                          <p className="num truncate text-xs text-muted-foreground select-all">
                                                              {purchaseNo}
                                                          </p>
                                                      ) : null}
                                                  </div>
                                                  <Button
                                                      id={`${idPrefix}-rail-close`}
                                                      type="button"
                                                      variant="ghost"
                                                      size="icon-sm"
                                                      aria-label="收起采购单对照栏"
                                                      onClick={closeRail}
                                                  >
                                                      <XIcon aria-hidden="true" />
                                                  </Button>
                                              </div>
                                              <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-3">
                                                  {railOpen && order ? (
                                                      <PaperDocumentViewport
                                                          id={`${idPrefix}-paper`}
                                                          fitMode="width"
                                                          fitKey={
                                                              purchaseOrderId
                                                          }
                                                          role="region"
                                                          aria-label="采购单纸面"
                                                          tabIndex={0}
                                                          className="bg-transparent p-0"
                                                      >
                                                          <PurchaseOrderPaperDocument
                                                              order={order}
                                                          />
                                                      </PaperDocumentViewport>
                                                  ) : orderQuery.isPending ? (
                                                      <p
                                                          role="status"
                                                          className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground"
                                                      >
                                                          <Spinner aria-hidden="true" />
                                                          正在读取采购单…
                                                      </p>
                                                  ) : orderQuery.isError ? (
                                                      <BusinessFailureState
                                                          id={`${idPrefix}-paper-error`}
                                                          title="采购单读取失败"
                                                          error={
                                                              orderQuery.error
                                                          }
                                                          onRetry={() =>
                                                              void orderQuery.refetch()
                                                          }
                                                      />
                                                  ) : (
                                                      <p className="py-6 text-center text-xs text-muted-foreground">
                                                          暂时无法查看采购单。
                                                      </p>
                                                  )}
                                              </div>
                                          </div>
                                      ),
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
                                            supplier?.trim() !== title.trim()
                                                ? supplier
                                                : null,
                                            purchaseNo && canReadOrder ? (
                                                <Link
                                                    key="purchase-number"
                                                    id={`${idPrefix}-purchase`}
                                                    href={purchaseHref}
                                                    className="num select-all underline decoration-border underline-offset-4 hover:decoration-foreground"
                                                >
                                                    {purchaseNo}
                                                </Link>
                                            ) : purchaseNo ? (
                                                <span
                                                    key="purchase-number"
                                                    className="num select-all"
                                                >
                                                    {purchaseNo}
                                                </span>
                                            ) : null,
                                            owner
                                                ? `采购负责人 ${owner}`
                                                : null,
                                            item.dueAt && item.dueAtLabel ? (
                                                <span
                                                    key="deadline"
                                                    className="num"
                                                >
                                                    截止 {item.dueAtLabel}
                                                </span>
                                            ) : order?.header.expectedDate ? (
                                                <span
                                                    key="delivery-date"
                                                    className="num"
                                                >
                                                    预计交付{" "}
                                                    {order.header.expectedDate}
                                                </span>
                                            ) : null,
                                        ]}
                                    />
                                }
                            >
                                {canReadOrder ? (
                                    inspectorMode ? (
                                        <Link
                                            id={`${idPrefix}-preview`}
                                            href={purchaseHref}
                                            className={buttonVariants({
                                                variant: "ghost",
                                                size: "sm",
                                            })}
                                        >
                                            <FileTextIcon
                                                aria-hidden="true"
                                                data-icon="inline-start"
                                            />
                                            查看采购单
                                            <ArrowUpRightIcon
                                                aria-hidden="true"
                                                data-icon="inline-end"
                                            />
                                        </Link>
                                    ) : (
                                        <Button
                                            ref={toggleRef}
                                            id={`${idPrefix}-preview`}
                                            type="button"
                                            variant={
                                                railExpanded
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            size="sm"
                                            aria-expanded={
                                                useRail
                                                    ? railExpanded
                                                    : undefined
                                            }
                                            aria-haspopup={
                                                useRail ? undefined : "dialog"
                                            }
                                            onClick={() =>
                                                useRail
                                                    ? setRailOpen(
                                                          (open) => !open,
                                                      )
                                                    : setPreviewOpen(true)
                                            }
                                        >
                                            {railExpanded ? (
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
                                            {railExpanded
                                                ? "收起采购单"
                                                : "对照采购单"}
                                        </Button>
                                    )
                                ) : null}
                            </WorkspaceTaskIdentityHeader>
                        }
                        aria-label="当前采购依据变更任务"
                    >
                        {body}
                        {canReadOrder && !inspectorMode && !useRail ? (
                            <WorkspaceDocumentPaperDialog
                                target={
                                    previewOpen
                                        ? {
                                              kind: "purchase_order",
                                              objectId: purchaseOrderId,
                                              title: purchaseNo,
                                          }
                                        : null
                                }
                                open={previewOpen}
                                onOpenChange={setPreviewOpen}
                                closeButtonId={`${idPrefix}-paper-close`}
                            />
                        ) : null}
                    </WorkspaceProcessingPane>
                )}
            />
        </div>
    )
}
