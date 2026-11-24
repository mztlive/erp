"use client"

import { useEffect, useRef, useState } from "react"
import Link from "next/link"
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
    useWorkspaceInspectorMode,
    useWorkspaceTaskPage,
} from "@/components/business"
import { Button, buttonVariants } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { PurchaseOrderPaperDocument } from "@/features/purchase-orders/components/purchase-order-paper-document"
import { PurchasePaymentArrangement } from "@/features/purchase-orders/components/purchase-payment-arrangement"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { usePaymentArrangement } from "@/features/purchase-orders/hooks/use-payment-arrangement"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentPaperDialog } from "./workspace-document-paper-dialog"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

type Props = {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}

export function WorkspacePurchasePaymentArrangementTask(props: Props) {
    return <PaymentArrangementTaskPane key={props.item.workItemId} {...props} />
}

function PaymentArrangementTaskPane({ item, onTaskCompleted }: Props) {
    const root = useRef<HTMLDivElement>(null)
    const toggleRef = useRef<HTMLButtonElement>(null)
    const [wide, setWide] = useState(false)
    const [referenceOpen, setReferenceOpen] = useState(false)
    const fullPage = useWorkspaceTaskPage()
    const inspector = useWorkspaceInspectorMode()
    const profile = useAccountProfileQuery()
    const canReference = hasPermission(
        profile.data?.permissions,
        "purchase_order:detail",
    )
    const arrangement = usePaymentArrangement(item.businessObjectId)
    const purchase = usePurchaseOrderCenterQuery(
        canReference ? item.businessObjectId : "",
    )
    const order = purchase.data
    const currentOrder =
        order?.identity.purchaseOrderId === item.businessObjectId &&
        order.identity.currentRevisionId === item.subjectVersion
            ? order
            : undefined
    const title = item.workItemTypeLabel || "采购付款待安排"
    const stableNumber = stripDocumentNumberPrefix(item.stableNumber)
    const purchaseOrderNo =
        currentOrder?.identity.purchaseNo ??
        (stableNumber !== title &&
        stableNumber !== "单号待补全" &&
        stableNumber !== "业务单号待补全"
            ? stableNumber
            : "")
    const supplier =
        currentOrder?.header.supplierSnapshot || item.counterpartyName?.trim()
    const paymentTerm =
        arrangement.data?.payment_term_label ||
        currentOrder?.header.paymentTermLabel
    const idPrefix = `workspace-purchase-payment-arrangement-${toAutomationIdSegment(item.workItemId)}`
    const canSplit = fullPage && !inspector && wide
    const showRail = canReference && canSplit
    const reference = {
        businessObjectType: "purchase_order",
        businessObjectId: item.businessObjectId,
        subjectVersion: item.subjectVersion,
    }
    const processBlocker =
        item.status === "OPEN"
            ? item.actionBlockers
                  .filter((blocker) => blocker.action === "PROCESS")
                  .map((blocker) => blocker.message)
                  .join("；")
            : undefined

    useEffect(() => {
        if (!fullPage || inspector || !root.current) return
        const target =
            root.current.closest<HTMLElement>("#main-content") ?? root.current
        const sync = () => {
            const nextWide = target.clientWidth >= 960
            setWide(nextWide)
            if (!nextWide) setReferenceOpen(false)
        }
        sync()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(sync)
        observer.observe(target)
        return () => observer.disconnect()
    }, [fullPage, inspector])

    useEffect(() => {
        if (!canReference || !fullPage || inspector) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") setReferenceOpen((open) => !open)
        })
    }, [canReference, fullPage, inspector])

    const closeRail = () => {
        setReferenceOpen(false)
        toggleRef.current?.focus()
    }
    const orderHref = `/procurement/orders/${encodeURIComponent(item.businessObjectId)}`

    return (
        <div ref={root} className="h-full min-h-0 min-w-0 bg-card">
            <PurchasePaymentArrangement
                idPrefix={idPrefix}
                purchaseOrderId={item.businessObjectId}
                purchaseOrderNo={purchaseOrderNo || undefined}
                workItemId={item.workItemId}
                costMasked={currentOrder?.currentContent.costMasked}
                identityInHeader={!inspector}
                taskBlocker={processBlocker}
                onTaskCompleted={
                    item.status === "OPEN" ? onTaskCompleted : undefined
                }
                renderSurface={(body) => (
                    <WorkspaceProcessingPane
                        archetype="action"
                        className="@container/document"
                        idPrefix={idPrefix}
                        aria-label="采购付款安排任务"
                        header={
                            <WorkspaceTaskIdentityHeader
                                item={item}
                                title={title}
                                compact={!fullPage}
                                subtitle={
                                    <DetailWorkspaceMeta
                                        items={[
                                            supplier !== title &&
                                            supplier !== purchaseOrderNo
                                                ? supplier
                                                : null,
                                            purchaseOrderNo ? (
                                                <span
                                                    key="number"
                                                    className="num select-all"
                                                    data-workspace-document-number={
                                                        purchaseOrderNo
                                                    }
                                                >
                                                    {purchaseOrderNo}
                                                </span>
                                            ) : null,
                                            paymentTerm || null,
                                            item.dueAt && item.dueAtLabel ? (
                                                <span
                                                    key="deadline"
                                                    className="num"
                                                >
                                                    截止 {item.dueAtLabel}
                                                </span>
                                            ) : null,
                                        ]}
                                    />
                                }
                            >
                                {canReference &&
                                    (inspector ? (
                                        <Link
                                            id={`${idPrefix}-reference`}
                                            href={orderHref}
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
                                            id={`${idPrefix}-reference`}
                                            type="button"
                                            variant={
                                                showRail && referenceOpen
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            size="sm"
                                            aria-expanded={
                                                showRail
                                                    ? referenceOpen
                                                    : undefined
                                            }
                                            aria-haspopup={
                                                !showRail ? "dialog" : undefined
                                            }
                                            onClick={() =>
                                                setReferenceOpen(
                                                    (open) => !open,
                                                )
                                            }
                                        >
                                            {showRail && referenceOpen ? (
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
                                            {showRail && referenceOpen
                                                ? "收起采购单"
                                                : "对照采购单"}
                                        </Button>
                                    ))}
                            </WorkspaceTaskIdentityHeader>
                        }
                        contextRail={
                            showRail
                                ? {
                                      collapsible: true,
                                      resizable: true,
                                      allowOnFullWidth: true,
                                      defaultWidthPercent: 38,
                                      minWidthPercent: 26,
                                      maxWidthPercent: 52,
                                      storageKey:
                                          "workspace-purchase-payment-arrangement-rail-width",
                                      collapsed: !referenceOpen,
                                      onCollapsedChange: (collapsed) =>
                                          setReferenceOpen(!collapsed),
                                      label: "对照采购单",
                                      content: referenceOpen ? (
                                          <PurchaseOrderReferenceRail
                                              query={purchase}
                                              order={currentOrder}
                                              purchaseOrderNo={purchaseOrderNo}
                                              idPrefix={idPrefix}
                                              onClose={closeRail}
                                          />
                                      ) : null,
                                  }
                                : undefined
                        }
                    >
                        {body}
                        {canReference &&
                            !inspector &&
                            !showRail &&
                            referenceOpen && (
                                <WorkspaceDocumentPaperDialog
                                    target={{
                                        kind: "purchase_order",
                                        objectId: item.businessObjectId,
                                        title: purchaseOrderNo || "采购单",
                                    }}
                                    reference={reference}
                                    open
                                    onOpenChange={setReferenceOpen}
                                    closeButtonId={`${idPrefix}-reference-dialog-close`}
                                />
                            )}
                    </WorkspaceProcessingPane>
                )}
            />
        </div>
    )
}

function PurchaseOrderReferenceRail({
    query,
    order,
    purchaseOrderNo,
    idPrefix,
    onClose,
}: {
    query: ReturnType<typeof usePurchaseOrderCenterQuery>
    order: ReturnType<typeof usePurchaseOrderCenterQuery>["data"]
    purchaseOrderNo: string
    idPrefix: string
    onClose: () => void
}) {
    return (
        <div className="flex h-full min-h-0 min-w-0 flex-col bg-card">
            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2.5 pr-1">
                <div className="min-w-0">
                    <h2 className="truncate text-body-compact font-semibold">
                        对照采购单
                    </h2>
                    {purchaseOrderNo ? (
                        <p className="num truncate text-xs text-muted-foreground select-all">
                            {purchaseOrderNo}
                        </p>
                    ) : null}
                </div>
                <Button
                    id={`${idPrefix}-reference-rail-close`}
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="收起采购单对照栏"
                    onClick={onClose}
                >
                    <XIcon aria-hidden="true" />
                </Button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-3">
                {query.isPending ? (
                    <p
                        role="status"
                        className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground"
                    >
                        <Spinner aria-hidden="true" />
                        正在读取采购单…
                    </p>
                ) : query.isError ? (
                    <BusinessFailureState
                        id={`${idPrefix}-reference-error`}
                        title="采购单读取失败"
                        error={query.error}
                        onRetry={() => void query.refetch()}
                    />
                ) : order ? (
                    <PaperDocumentViewport
                        id={`${idPrefix}-reference-paper`}
                        className="min-w-0 overflow-x-hidden bg-transparent p-0"
                        fitMode="width"
                        fitKey={order.identity.currentRevisionId}
                        role="region"
                        aria-label="采购单纸面"
                        tabIndex={0}
                    >
                        <PurchaseOrderPaperDocument order={order} />
                    </PaperDocumentViewport>
                ) : (
                    <p
                        role="alert"
                        className="py-3 text-xs text-warning-soft-foreground"
                    >
                        关联采购单版本已变化，请刷新任务后核对。
                    </p>
                )}
            </div>
        </div>
    )
}
