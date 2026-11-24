"use client"

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"

import { DetailWorkspaceMeta } from "@/components/business"
import {
    useWorkspaceInspectorMode,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { SupplierOrderCenterPage } from "@/features/supplier-orders/pages/supplier-order-center-page"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    dispatchWorkspaceHotkeyAction,
    subscribeWorkspaceHotkeyAction,
} from "@/lib/workspace-hotkeys"

import { useWorkspaceHotkeys } from "../hooks/use-workspace-hotkeys"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import {
    WorkspaceCompactTaskHeader,
    WorkspaceTaskIdentityHeader,
} from "./workspace-task-identity-header"

type InvestigationProps = {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}

type RailTab = "diagnostics" | "fulfillment"

export function WorkspaceSupplierInvestigationTask(props: InvestigationProps) {
    return <SupplierInvestigation key={props.item.workItemId} {...props} />
}

function SupplierInvestigation({ item, onTaskCompleted }: InvestigationProps) {
    const inspectorMode = useWorkspaceInspectorMode()
    const fullPage = useWorkspaceTaskPage() && !inspectorMode
    const [collapsed, setCollapsed] = useState(!fullPage)
    const [tab, setTab] = useState<RailTab>("diagnostics")
    const [diagnosticsPulse, setDiagnosticsPulse] = useState(false)
    const pulseTimer = useRef<number | null>(null)
    const idPrefix = `workspace-supplier-investigation-${toAutomationIdSegment(item.workItemId)}`

    useEffect(
        () => () => {
            if (pulseTimer.current !== null) {
                window.clearTimeout(pulseTimer.current)
            }
        },
        [],
    )

    const openRailTab = useCallback(
        (next: RailTab, pulse = false) => {
            setTab(next)
            setCollapsed(false)
            if (pulse && next === "diagnostics") {
                setDiagnosticsPulse(true)
                if (pulseTimer.current !== null) {
                    window.clearTimeout(pulseTimer.current)
                }
                pulseTimer.current = window.setTimeout(
                    () => setDiagnosticsPulse(false),
                    2200,
                )
            }
            window.requestAnimationFrame(() => {
                const trigger = document.getElementById(
                    `${idPrefix}-tab-${next}`,
                )
                if (!trigger) return
                const reduceMotion = window.matchMedia(
                    "(prefers-reduced-motion: reduce)",
                ).matches
                trigger.focus({ preventScroll: true })
                trigger.scrollIntoView({
                    block: "nearest",
                    behavior: reduceMotion ? "auto" : "smooth",
                })
            })
        },
        [idPrefix],
    )

    useWorkspaceHotkeys({
        enabled: !fullPage,
        enableCheatSheet: false,
        onToggleRail: () => dispatchWorkspaceHotkeyAction("toggle-rail"),
        onHistory: () => dispatchWorkspaceHotkeyAction("history"),
    })

    useEffect(() => {
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") setCollapsed((current) => !current)
            if (action === "history") openRailTab("fulfillment")
        })
    }, [openRailTab])

    function openDiagnostics() {
        openRailTab("diagnostics", true)
    }

    // 交接和关闭沿用完整页共用确认框，轻作业面由宿主的完整页面入口承接。
    const headerItem = inspectorMode
        ? {
              ...item,
              allowedActions: item.allowedActions.filter(
                  (action) => action !== "REASSIGN" && action !== "CLOSE",
              ),
          }
        : item

    return (
        <SupplierOrderCenterPage
            supplierOrderId={item.businessObjectId}
            workItemId={item.workItemId}
            onTaskCompleted={onTaskCompleted}
            embedded={{
                idPrefix,
                onOpenDiagnostics: openDiagnostics,
                render: ({ body, order, diagnostics, fulfillment }) => {
                    const parties = [
                        ...new Set(
                            [order?.supplierName, order?.connectionCode]
                                .map((name) => name?.trim())
                                .filter(
                                    (name) => name && name !== order?.orderNo,
                                ),
                        ),
                    ]
                    const meta = order ? (
                        <DetailWorkspaceMeta
                            items={[
                                <span key="order" className="num select-all">
                                    {order.orderNo}
                                </span>,
                                parties.join(" / "),
                                item.dueAt && item.dueAtLabel ? (
                                    <span key="deadline" className="num">
                                        截止 {item.dueAtLabel}
                                    </span>
                                ) : null,
                            ]}
                        />
                    ) : null

                    return (
                        <WorkspaceProcessingPane
                            archetype="investigation"
                            idPrefix={idPrefix}
                            className={
                                !fullPage ? "@container/workspace" : undefined
                            }
                            aria-label="当前供应商履约异常调查任务"
                            header={
                                order ? (
                                    <WorkspaceTaskIdentityHeader
                                        compact
                                        item={headerItem}
                                        title="供应商履约异常调查"
                                        subtitle={meta}
                                    />
                                ) : (
                                    <WorkspaceCompactTaskHeader title="业务详情" />
                                )
                            }
                            headerSupplement={
                                inspectorMode && meta ? (
                                    <div className="min-w-0 py-1.5">{meta}</div>
                                ) : undefined
                            }
                            contextRail={
                                order
                                    ? {
                                          label: "供应商订单诊断与履约",
                                          collapsible: true,
                                          resizable: fullPage,
                                          defaultWidthPercent: 30,
                                          minWidthPercent: 24,
                                          maxWidthPercent: 52,
                                          storageKey:
                                              "erp.workspace.investigationRailWidth",
                                          allowOnFullWidth: true,
                                          collapsed,
                                          onCollapsedChange: setCollapsed,
                                          content: (
                                              <SupplierInvestigationRail
                                                  idPrefix={idPrefix}
                                                  tab={tab}
                                                  setTab={setTab}
                                                  diagnostics={diagnostics}
                                                  fulfillment={fulfillment}
                                                  diagnosticsPulse={
                                                      diagnosticsPulse
                                                  }
                                              />
                                          ),
                                      }
                                    : undefined
                            }
                        >
                            {body}
                        </WorkspaceProcessingPane>
                    )
                },
            }}
        />
    )
}

function SupplierInvestigationRail({
    idPrefix,
    tab,
    setTab,
    diagnostics,
    fulfillment,
    diagnosticsPulse,
}: {
    idPrefix: string
    tab: RailTab
    setTab: (tab: RailTab) => void
    diagnostics: ReactNode
    fulfillment: ReactNode
    diagnosticsPulse: boolean
}) {
    return (
        <Tabs
            value={tab}
            onValueChange={setTab}
            className="min-h-0 min-w-0 flex-1 gap-0"
        >
            <TabsList
                variant="line"
                aria-label="调查依据"
                className="w-full shrink-0 flex-wrap gap-x-4 gap-y-0 border-b border-border/60"
            >
                <TabsTrigger
                    id={`${idPrefix}-tab-diagnostics`}
                    value="diagnostics"
                >
                    同步诊断与日志
                </TabsTrigger>
                <TabsTrigger
                    id={`${idPrefix}-tab-fulfillment`}
                    value="fulfillment"
                >
                    履约跟踪
                </TabsTrigger>
            </TabsList>
            <TabsContent
                id={`${idPrefix}-panel-diagnostics`}
                value="diagnostics"
                keepMounted
                className={cn(
                    "min-h-0 min-w-0 overflow-y-auto wrap-anywhere",
                    diagnosticsPulse &&
                        "rounded-md bg-info-soft/25 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                )}
            >
                {diagnostics}
            </TabsContent>
            <TabsContent
                id={`${idPrefix}-panel-fulfillment`}
                value="fulfillment"
                className="min-h-0 min-w-0 overflow-y-auto wrap-anywhere"
            >
                {fulfillment}
            </TabsContent>
        </Tabs>
    )
}
