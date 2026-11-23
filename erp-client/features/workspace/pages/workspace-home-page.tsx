"use client"

import { useCallback, useMemo, useState } from "react"
import { usePathname } from "@/lib/navigation"
import { WORKSPACE_QUEUE_ROUTES } from "@/lib/workspace-queue-routes"
import { DownloadIcon, RefreshCwIcon } from "lucide-react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { BusinessFailureState, PageScaffold } from "@/components/business"
import {
    ListWorkspacePage,
    listWorkspaceStyles,
    type ListWorkspaceViewItem,
} from "@/components/business/list-workspace"
import {
    WorkspaceQueueLayoutToggle,
    useWorkspaceQueueLayout,
} from "@/components/business/workspace-split-queue"
import type { ApprovalInstanceListView } from "@/features/approval-workflow/api"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { excelExportText } from "@/lib/ui-text"
import { useScopeExcelExport } from "@/lib/use-scope-list-export"
import { WorkspaceQueuePanel } from "../components/workspace-queue-panel"
import { buildWorkspaceBucketViews } from "../components/workspace-type-nav"
import { useWorkspaceHome } from "../hooks/use-workspace-home"
import {
    WorkspaceHotkeyCheatSheetButton,
    dispatchWorkspaceHotkeyAction,
    useWorkspaceHotkeys,
} from "../hooks/use-workspace-hotkeys"
import { deriveWorkItemsFreshness } from "../lib/freshness"
import { usePaymentExport } from "../hooks/use-payment-export"
import {
    exportWorkspaceInvoiceTasksExcel,
    invoiceExecutionExportQuery,
} from "../lib/workspace-invoice"
import { RoleWorkbenchEntry } from "./role-workbench-entry"

export function WorkspaceHomePage() {
    const pathname = usePathname()
    const profile = useAccountProfileQuery().data
    if (pathname === "/workspace") return <RoleWorkbenchEntry />
    return (
        <WorkspaceTaskHome
            key={`${profile?.userid}:${profile?.policy_version}:${profile?.organization_version}:${profile?.permissions.join(",")}`}
        />
    )
}

function WorkspaceTaskHome() {
    return <WorkspaceHomeView home={useWorkspaceHome()} />
}

const APPROVAL_VIEW_META: Readonly<
    Record<ApprovalInstanceListView, { title: string; tabLabel: string }>
> = {
    started: { title: "我发起的审批", tabLabel: "我发起的" },
    mine: { title: "待我审批", tabLabel: "待我审批" },
    blocked: { title: "受阻治理", tabLabel: "受阻治理" },
    managed: { title: "管理范围审批", tabLabel: "管理范围" },
}

/** 统一待办保持全部责任范围；顶层视图内联至 44px 单行页头，消除双层堆叠。 */
export function WorkspaceHomeView({
    home,
}: {
    home: ReturnType<typeof useWorkspaceHome>
}) {
    const paymentExport = usePaymentExport()
    const invoiceExport = useScopeExcelExport(exportWorkspaceInvoiceTasksExcel)
    const invoiceExportQuery = invoiceExecutionExportQuery(home.queryInput)
    const [layoutMode, setLayoutMode] = useWorkspaceQueueLayout("table")
    const [activeInspectorId, setActiveInspectorId] = useState<string | null>(
        null,
    )
    const [selectedIds, setSelectedIds] = useState<string[]>([])
    const queueItems = useMemo(() => home.view?.items ?? [], [home.view?.items])

    const scrollItemIntoView = useCallback((itemId: string) => {
        window.requestAnimationFrame(() => {
            const escaped = CSS.escape(itemId)
            const target = document.querySelector<HTMLElement>(
                `[data-split-item-id="${escaped}"], [data-row-id="${escaped}"]`,
            )
            target?.scrollIntoView({ block: "nearest" })
        })
    }, [])

    const handleNextItem = useCallback(() => {
        if (queueItems.length === 0) return
        const currentIndex = activeInspectorId
            ? queueItems.findIndex(
                  (entry) => entry.workItemId === activeInspectorId,
              )
            : -1
        const nextIndex =
            currentIndex < 0
                ? 0
                : Math.min(currentIndex + 1, queueItems.length - 1)
        const nextId = queueItems[nextIndex]?.workItemId
        if (!nextId) return
        setActiveInspectorId(nextId)
        scrollItemIntoView(nextId)
    }, [activeInspectorId, queueItems, scrollItemIntoView])

    const handlePrevItem = useCallback(() => {
        if (queueItems.length === 0) return
        const currentIndex = activeInspectorId
            ? queueItems.findIndex(
                  (entry) => entry.workItemId === activeInspectorId,
              )
            : -1
        const prevIndex = currentIndex <= 0 ? 0 : currentIndex - 1
        const prevId = queueItems[prevIndex]?.workItemId
        if (!prevId) return
        setActiveInspectorId(prevId)
        scrollItemIntoView(prevId)
    }, [activeInspectorId, queueItems, scrollItemIntoView])

    const handleFocusSearch = useCallback(() => {
        const input = document.getElementById(
            "workspace-queue-toolbar-search-input",
        )
        if (input instanceof HTMLInputElement) {
            input.focus()
            input.select()
        }
    }, [])

    const handleEscape = useCallback(() => {
        if (activeInspectorId) {
            setActiveInspectorId(null)
            return
        }
        if (selectedIds.length > 0) {
            setSelectedIds([])
        }
    }, [activeInspectorId, selectedIds.length])

    useWorkspaceHotkeys({
        enabled: !home.accountProfileQuery.isError,
        onNext: handleNextItem,
        onPrev: handlePrevItem,
        onApprove: activeInspectorId
            ? () => dispatchWorkspaceHotkeyAction("approve")
            : undefined,
        onReject: activeInspectorId
            ? () => dispatchWorkspaceHotkeyAction("reject")
            : undefined,
        onFocusSearch: handleFocusSearch,
        onEscape:
            activeInspectorId || selectedIds.length > 0
                ? handleEscape
                : undefined,
    })
    const permissions = home.accountProfileQuery.data?.permissions
    const canReadApprovals = hasPermission(
        permissions,
        "approval_instance:read",
    )
    const canListWorkItems = hasPermission(permissions, "work_item:list")
    const canExportPayments = [
        "work_item:list",
        "purchase_payment_request:detail",
        "purchase_payment_request:pay",
        "party_bank_account:reveal",
    ].every((permission) => hasPermission(permissions, permission))
    const paymentQueue =
        home.urlState.view === "inbox" &&
        home.urlState.filters?.item_types?.length === 1 &&
        home.urlState.filters.item_types[0] ===
            "task:SUPPLIER_PAYMENT_EXECUTION"
    const freshness = home.view
        ? deriveWorkItemsFreshness(home.view.freshness, {
              refreshing: home.refreshing,
          })
        : undefined
    const isApprovalsRoute = home.urlState.view === "started"

    const headerViewsConfig = (() => {
        if (isApprovalsRoute) {
            if (!canReadApprovals) return undefined
            const items: ListWorkspaceViewItem[] = (
                ["started", "mine", "blocked", "managed"] as const
            )
                .filter((viewKey) => {
                    if (viewKey === "started" || viewKey === "mine") {
                        return true
                    }
                    const available =
                        home.view?.approvalViewAvailability?.[viewKey]
                    return available !== false || home.approvalView === viewKey
                })
                .map((viewKey) => ({
                    id: `workspace-approval-view-${viewKey}`,
                    label: APPROVAL_VIEW_META[viewKey].tabLabel,
                    count: home.view?.approvalViewCounts?.[viewKey],
                    active: home.approvalView === viewKey,
                    onClick: () => home.onApprovalViewChange(viewKey),
                }))
            return items.length >= 2
                ? { ariaLabel: "审批视图", items }
                : undefined
        }
        if (!canListWorkItems) return undefined
        return {
            ariaLabel: "事项分类",
            items: buildWorkspaceBucketViews({
                selectedTypes: home.urlState.filters?.item_types,
                counts: home.queueCounts.counts,
                onSelectItemTypes: (item_types) =>
                    home.applyFilters(home.urlState.query ?? "", {
                        ...home.urlState.filters,
                        item_types: item_types.length ? item_types : undefined,
                    }),
            }),
        }
    })()

    const headerTitle =
        home.urlState.view === "managed"
            ? "范围内待办"
            : isApprovalsRoute
              ? APPROVAL_VIEW_META[home.approvalView].title
              : WORKSPACE_QUEUE_ROUTES[home.urlState.view].title

    if (home.accountProfileQuery.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    idPrefix="workspace-account-profile-failure"
                    error={home.accountProfileQuery.error}
                    readContext={{ subject: "账号资料" }}
                    onRetry={home.refresh}
                />
            </PageScaffold>
        )

    return (
        <ListWorkspacePage
            title={headerTitle}
            views={headerViewsConfig}
            viewsPlacement="inline"
            className="min-h-0 overflow-auto [&>[data-slot=list-workspace-header]]:min-h-11 [&>[data-slot=list-workspace-header]]:md:min-h-11 [&_[data-slot=list-workspace-views][data-variant=inline]_button]:md:min-h-11 [&_[data-slot=list-workspace-views][data-variant=inline]_a]:md:min-h-11"
            headerActions={
                <>
                    {!home.dashboardQuery.isError && home.view != null && (
                        <span
                            role="status"
                            className="min-w-0 px-1 text-xs text-muted-foreground"
                        >
                            <span className="num">
                                共 {home.view.total.toLocaleString("zh-CN")} 条
                            </span>
                            {home.refreshing ? (
                                " · 更新中…"
                            ) : freshness ? (
                                <>
                                    {" · "}
                                    <time
                                        className="num"
                                        dateTime={freshness.dateTime}
                                    >
                                        {freshness.updatedAtLabel}
                                    </time>
                                </>
                            ) : null}
                        </span>
                    )}
                    {paymentQueue && canExportPayments && (
                        <Button
                            id="workspace-payment-export"
                            type="button"
                            variant="ghost"
                            className={listWorkspaceStyles.quietButton}
                            disabled={
                                paymentExport.isPending ||
                                home.refreshing ||
                                home.dashboardQuery.isError ||
                                !home.view?.total
                            }
                            onClick={() => paymentExport.mutate(home.urlState)}
                            title="导出当前筛选下的全部待付款记录"
                        >
                            <DownloadIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {paymentExport.isPending
                                ? "正在导出…"
                                : "导出 Excel"}
                        </Button>
                    )}
                    {invoiceExportQuery && canListWorkItems && (
                        <LoadingButton
                            id="workspace-invoice-execution-export"
                            type="button"
                            variant="ghost"
                            className={listWorkspaceStyles.quietButton}
                            loading={invoiceExport.isPending}
                            disabled={
                                home.refreshing ||
                                home.dashboardQuery.isError ||
                                !home.view?.total
                            }
                            onClick={() =>
                                invoiceExport.mutate(invoiceExportQuery)
                            }
                            title="导出当前筛选下的全部开票执行记录"
                        >
                            <DownloadIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {invoiceExport.isPending
                                ? excelExportText.pending
                                : excelExportText.action}
                        </LoadingButton>
                    )}
                    <WorkspaceQueueLayoutToggle
                        value={layoutMode}
                        onValueChange={setLayoutMode}
                        idPrefix="workspace-queue-layout"
                    />
                    <WorkspaceHotkeyCheatSheetButton
                        id="workspace-home-hotkeys"
                        scope="queue"
                    />
                    <Button
                        id="workspace-home-refresh"
                        type="button"
                        variant="ghost"
                        className={listWorkspaceStyles.quietButton}
                        disabled={home.refreshing}
                        onClick={home.refresh}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        刷新
                    </Button>
                </>
            }
        >
            <WorkspaceQueuePanel
                home={home}
                onSelectTask={home.onSelectTask}
                layoutMode={layoutMode}
                activeInspectorId={activeInspectorId}
                onActiveInspectorIdChange={setActiveInspectorId}
                selectedIds={selectedIds}
                onSelectedIdsChange={setSelectedIds}
            />
        </ListWorkspacePage>
    )
}
