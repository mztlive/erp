"use client"

import { usePathname } from "@/lib/navigation"
import { WORKSPACE_QUEUE_ROUTES } from "@/lib/workspace-queue-routes"
import { DownloadIcon, RefreshCwIcon } from "lucide-react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { BusinessFailureState, PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { excelExportText } from "@/lib/ui-text"
import { useScopeExcelExport } from "@/lib/use-scope-list-export"
import { cn } from "@/lib/utils"
import { WorkspaceQueuePanel } from "../components/workspace-queue-panel"
import { useWorkspaceHome } from "../hooks/use-workspace-home"
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

/** 统一待办保持全部责任范围；岗位筛选只通过显式查询参数进入。 */
export function WorkspaceHomeView({
    home,
}: {
    home: ReturnType<typeof useWorkspaceHome>
}) {
    const paymentExport = usePaymentExport()
    const invoiceExport = useScopeExcelExport(exportWorkspaceInvoiceTasksExcel)
    const invoiceExportQuery = invoiceExecutionExportQuery(home.queryInput)
    const permissions = home.accountProfileQuery.data?.permissions
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
        <PageScaffold
            density="compact"
            className={cn(listWorkspaceStyles.page, "min-h-0 overflow-auto")}
        >
            <ListWorkspaceHeader
                className="min-h-12 flex-row flex-wrap items-center py-2 md:py-0 [&>div:last-child]:max-w-full [&>div:last-child]:flex-wrap [&>div:last-child]:max-md:w-auto"
                title={
                    home.urlState.view === "managed"
                        ? "范围内待办"
                        : WORKSPACE_QUEUE_ROUTES[home.urlState.view].title
                }
            >
                {!home.dashboardQuery.isError && (
                    <span
                        role="status"
                        className="min-w-0 px-1 text-xs text-muted-foreground"
                    >
                        {home.refreshing ? (
                            "正在更新…"
                        ) : freshness ? (
                            <time className="num" dateTime={freshness.dateTime}>
                                更新于 {freshness.updatedAtLabel}
                            </time>
                        ) : (
                            "正在查询"
                        )}
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
                        {paymentExport.isPending ? "正在导出…" : "导出 Excel"}
                    </Button>
                )}
                {invoiceExportQuery &&
                    hasPermission(permissions, "work_item:list") && (
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
            </ListWorkspaceHeader>
            <WorkspaceQueuePanel home={home} onSelectTask={home.onSelectTask} />
        </PageScaffold>
    )
}
