"use client"

import { useCallback, useEffect, useRef, useState } from "react"

import { DetailWorkspaceMeta } from "@/components/business"
import {
    useWorkspaceInspectorMode,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    SupplyExceptionTaskPanel,
    type SupplyExceptionReceipt,
} from "@/features/supplier-offerings/components/supply-exception-task-panel"
import {
    useSupplierOfferingQuery,
    useSupplierSupplyExceptionWorkItemQuery,
} from "@/features/supplier-offerings/hooks/queries"
import type { SupplierSupplyExceptionWorkItem } from "@/features/supplier-offerings/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import { displayBusinessText } from "@/lib/display-name"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import {
    dispatchWorkspaceHotkeyAction,
    subscribeWorkspaceHotkeyAction,
} from "@/lib/workspace-hotkeys"

import { useWorkspaceHotkeys } from "../hooks/use-workspace-hotkeys"
import type { WorkspaceWorkItem } from "../types"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import {
    WorkspaceCompactTaskHeader,
    WorkspaceTaskIdentityHeader,
} from "./workspace-task-identity-header"

type Props = {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}

type RailTab = "diagnostics" | "history"

export function WorkspaceSupplyExceptionTask(props: Props) {
    return <SupplyExceptionWorkbench key={props.item.workItemId} {...props} />
}

function SupplyExceptionWorkbench({ item, onTaskCompleted }: Props) {
    const inspectorMode = useWorkspaceInspectorMode()
    const fullPage = useWorkspaceTaskPage() && !inspectorMode
    const idPrefix = `workspace-supply-exception-${toAutomationIdSegment(item.workItemId)}`
    const [collapsed, setCollapsed] = useState(!fullPage)
    const [tab, setTab] = useState<RailTab>("diagnostics")
    const [diagnosticsPulse, setDiagnosticsPulse] = useState(false)
    const pulseTimer = useRef<number | null>(null)
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canReadTask = hasPermission(permissions, "work_item:detail")
    const canReadOffering = hasPermission(permissions, "supplier_offering:list")
    const taskQuery = useSupplierSupplyExceptionWorkItemQuery(
        canReadTask ? item.workItemId : undefined,
    )
    const task = canReadTask && !taskQuery.isError ? taskQuery.data : undefined
    const offeringQuery = useSupplierOfferingQuery(
        canReadOffering ? (task?.businessObjectId ?? "") : "",
    )
    const offering =
        canReadOffering && !offeringQuery.isError
            ? offeringQuery.data
            : undefined

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
        enabled: !fullPage && Boolean(task),
        enableCheatSheet: false,
        onToggleRail: () => dispatchWorkspaceHotkeyAction("toggle-rail"),
        onHistory: () => dispatchWorkspaceHotkeyAction("history"),
    })

    useEffect(() => {
        if (!canReadTask) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") setCollapsed((current) => !current)
            if (action === "history") openRailTab("history")
        })
    }, [canReadTask, openRailTab])

    const offeringCode = offering?.supplier_sku_code
    const skuCode = offering?.sku_no
    const supplierName =
        offering?.supplier_name ||
        task?.counterpartyLabel ||
        item.counterpartyName
    const fallbackCode = displayBusinessText(
        stripDocumentNumberPrefix(item.stableNumber),
        item.workItemTypeLabel,
        item.businessObjectId,
        item.workItemId,
    )
    const codes = [
        ...new Set(
            [offeringCode, skuCode].map((code) => code?.trim()).filter(Boolean),
        ),
    ]
    const headerCodes = codes.length > 0 ? codes : [fallbackCode]
    const meta = canReadTask ? (
        <DetailWorkspaceMeta
            className="min-w-0 max-w-full"
            items={[
                ...(canReadOffering ? headerCodes : []).map((code) =>
                    code ? (
                        <span key={code} className="num select-all">
                            {code}
                        </span>
                    ) : null,
                ),
                canReadOffering &&
                supplierName &&
                !headerCodes.includes(supplierName)
                    ? supplierName
                    : null,
                item.dueAt && item.dueAtLabel ? (
                    <span key="due" className="num">
                        截止 {item.dueAtLabel}
                    </span>
                ) : null,
            ]}
        />
    ) : null

    // 交接和关闭沿用完整页共用确认框，轻作业面由宿主的完整页面入口承接，避免抽屉叠弹窗。
    const headerItem = inspectorMode
        ? {
              ...item,
              allowedActions: item.allowedActions.filter(
                  (action) => action !== "REASSIGN" && action !== "CLOSE",
              ),
          }
        : item

    return (
        <SupplyExceptionTaskPanel
            idPrefix={idPrefix}
            workItemId={item.workItemId}
            task={task}
            offering={offering}
            isPending={
                account.isPending || (canReadTask && taskQuery.isPending)
            }
            error={account.error ?? taskQuery.error}
            onRetry={async () => {
                const query = canReadTask ? taskQuery : account
                await query.refetch({ throwOnError: true })
            }}
            embedded
            onTaskCompleted={onTaskCompleted}
            onOpenDiagnostics={
                task ? () => openRailTab("diagnostics", true) : undefined
            }
            renderWorkbench={({ body, receipt }) => (
                <WorkspaceProcessingPane
                    archetype="investigation"
                    idPrefix={idPrefix}
                    className={!fullPage ? "@container/workspace" : undefined}
                    aria-label="当前供应停止核对任务"
                    header={
                        canReadTask ? (
                            <WorkspaceTaskIdentityHeader
                                item={headerItem}
                                title={
                                    canReadOffering
                                        ? "供应停止异常核对"
                                        : "业务详情"
                                }
                                subtitle={meta}
                                compact
                            />
                        ) : (
                            <WorkspaceCompactTaskHeader title="业务详情" />
                        )
                    }
                    headerSupplement={
                        inspectorMode && meta ? (
                            <div className="min-w-0 py-1.5">{meta}</div>
                        ) : null
                    }
                    contextRail={
                        task || (canReadTask && receipt)
                            ? {
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
                                  label: "停供诊断与核对记录",
                                  content: (
                                      <SupplyExceptionRail
                                          idPrefix={idPrefix}
                                          tab={task ? tab : "history"}
                                          onTabChange={setTab}
                                          task={task}
                                          registeredAt={
                                              unixSecondsToIsoOrEmpty(
                                                  task?.createdAt,
                                              ) || item.createdAt
                                          }
                                          offeringName={offering?.sku_name}
                                          receipt={receipt}
                                          diagnosticsPulse={diagnosticsPulse}
                                      />
                                  ),
                              }
                            : undefined
                    }
                >
                    {body}
                </WorkspaceProcessingPane>
            )}
        />
    )
}

function SupplyExceptionRail({
    idPrefix,
    tab,
    onTabChange,
    task,
    registeredAt,
    offeringName,
    receipt,
    diagnosticsPulse,
}: {
    idPrefix: string
    tab: RailTab
    onTabChange: (tab: RailTab) => void
    task?: SupplierSupplyExceptionWorkItem
    registeredAt: string
    offeringName?: string | null
    receipt?: SupplyExceptionReceipt
    diagnosticsPulse: boolean
}) {
    const blockers = [
        ...new Set(
            [
                task?.processingBlocker?.message,
                ...(task?.actionBlockers ?? []),
            ].filter(Boolean),
        ),
    ]

    return (
        <Tabs
            value={tab}
            onValueChange={onTabChange}
            className="min-h-0 min-w-0 flex-1 gap-0"
        >
            <TabsList
                variant="line"
                aria-label="停供核对依据"
                className="w-full shrink-0 flex-wrap gap-x-4 gap-y-0 border-b border-border/60"
            >
                {task ? (
                    <TabsTrigger
                        id={`${idPrefix}-tab-diagnostics`}
                        value="diagnostics"
                    >
                        影响诊断
                    </TabsTrigger>
                ) : null}
                <TabsTrigger id={`${idPrefix}-tab-history`} value="history">
                    核对记录
                </TabsTrigger>
            </TabsList>
            {task ? (
                <TabsContent
                    id={`${idPrefix}-diagnostics`}
                    value="diagnostics"
                    className={cn(
                        "min-h-0 min-w-0 overflow-y-auto py-4 wrap-anywhere",
                        diagnosticsPulse &&
                            "rounded-md bg-info-soft/25 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                    )}
                >
                    <dl className="space-y-3 text-body-compact">
                        {offeringName ? (
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    受影响商品
                                </dt>
                                <dd>{offeringName}</dd>
                            </div>
                        ) : null}
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                停供原因
                            </dt>
                            <dd>{task.reasonLabel}</dd>
                        </div>
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                影响范围
                            </dt>
                            <dd>{task.impactSummary}</dd>
                        </div>
                        {blockers.length > 0 ? (
                            <div>
                                <dt className="text-xs font-medium text-warning-soft-foreground">
                                    办理限制
                                </dt>
                                <dd>
                                    <ul className="list-disc space-y-1 pl-4">
                                        {blockers.map((blocker) => (
                                            <li key={blocker}>{blocker}</li>
                                        ))}
                                    </ul>
                                </dd>
                            </div>
                        ) : null}
                    </dl>
                </TabsContent>
            ) : null}
            <TabsContent
                id={`${idPrefix}-history`}
                value="history"
                className="min-h-0 min-w-0 overflow-y-auto py-4 wrap-anywhere"
            >
                <ol className="divide-y divide-border/60 text-body-compact">
                    {registeredAt ? (
                        <li className="space-y-0.5 pb-3">
                            <p className="font-medium">生成停供核对任务</p>
                            <time
                                dateTime={registeredAt}
                                className="num text-xs text-muted-foreground"
                            >
                                {formatDateTime(registeredAt)}
                            </time>
                        </li>
                    ) : null}
                    <li
                        className={
                            registeredAt ? "space-y-1 pt-3" : "space-y-1"
                        }
                    >
                        {receipt ? (
                            <>
                                <p className="font-medium">核对已完成</p>
                                <p className="num select-all">
                                    凭据 {receipt.evidenceReference}
                                </p>
                                <p className="whitespace-pre-wrap">
                                    {receipt.comment}
                                </p>
                            </>
                        ) : (
                            <p className="text-xs text-muted-foreground">
                                暂无已确认的核对凭据。
                            </p>
                        )}
                    </li>
                </ol>
            </TabsContent>
        </Tabs>
    )
}
