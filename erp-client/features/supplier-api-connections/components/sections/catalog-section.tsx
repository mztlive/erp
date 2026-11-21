"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import type { ReactNode } from "react"
import type { UseQueryResult } from "@tanstack/react-query"
import { useQueryClient } from "@tanstack/react-query"
import Link from "next/link"
import { HistoryIcon, RefreshCwIcon } from "lucide-react"
import {
    BusinessFailureState,
    ListWorkspacePage,
    listWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    SheetClose,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
    SheetTrigger,
} from "@/components/ui/sheet"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import type { CatalogResult } from "../../api/catalog"
import type { ConnectionCenterView } from "../../types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { buildSupplierCatalogHref } from "../../lib/url-state"
import { CatalogBrowser } from "../catalog/catalog-browser"
import { CatalogBatchHistory } from "../catalog/catalog-batch-history"
import { CatalogOnboarding } from "../catalog/catalog-onboarding"
import { CatalogSyncRecords } from "../catalog/catalog-sync-records"

export function CatalogSection({
    conn,
    query,
    syncing,
    canSync,
    blocker,
    onSync,
    settingsHref,
    feedback,
}: {
    conn: ConnectionCenterView
    feedback?: ReactNode
    settingsHref: string
    query: UseQueryResult<CatalogResult, Error>
    syncing: boolean
    canSync: boolean
    blocker?: string
    onSync: () => Promise<void>
}) {
    const account = useAccountProfileQuery()
    const queryClient = useQueryClient()
    const latest = query.data?.jobs[0]
    const syncInterrupted =
        latest?.stalled ||
        latest?.status === "failed" ||
        latest?.status === "cancelled"
    const syncLabel = syncing
        ? "正在同步…"
        : syncInterrupted
          ? "继续同步商品"
          : latest
            ? "同步最新商品"
            : "开始同步商品"
    const syncBlocker =
        !canSync && !syncing
            ? (blocker ?? "当前账号没有商品同步权限，请联系管理员。")
            : undefined
    const canReadCost = hasPermission(
        account.data?.permissions,
        "supplier_offering_cost:detail",
    )
    const canReview = [
        "supplier_api_connection:catalog_sync",
        "product:update",
        "supplier_offering:update",
    ].every((permission) =>
        hasPermission(account.data?.permissions, permission),
    )
    const adoptionBlocker = !conn.deploymentReady
        ? "供应商接口尚未配置，请联系管理员。"
        : conn.status === "FAULTED"
          ? "供应商接口处于故障状态，请检查连接配置。"
          : undefined
    const canAdopt =
        hasPermission(
            account.data?.permissions,
            "supplier_api_connection:catalog_sync",
        ) &&
        hasPermission(account.data?.permissions, "supplier_offering:create") &&
        hasPermission(
            account.data?.permissions,
            "supplier_offering_cost:detail",
        )
    const segment = toAutomationIdSegment(conn.connectionId)
    const workspace = {
        title: `${conn.supplier.name} · 商品库`,
        views: {
            ariaLabel: "供应商连接工作区",
            items: [
                {
                    id: `supplier-api-connection-${segment}-settings-nav`,
                    label: "平台设置",
                    href: settingsHref,
                    active: false,
                },
                {
                    id: `supplier-api-connection-${segment}-catalog-nav`,
                    label: "商品库",
                    href: buildSupplierCatalogHref(conn.connectionId),
                    active: true,
                },
            ],
        },
        alerts: feedback,
        headerActions: (
            <>
                {canReadCost &&
                hasPermission(
                    account.data?.permissions,
                    "supplier_api_connection:catalog_sync",
                ) ? (
                    <CatalogBatchHistory
                        key={conn.connectionId}
                        connectionId={conn.connectionId}
                        supplierName={conn.supplier.name}
                    />
                ) : null}
                <SheetTrigger
                    id="supplier-catalog-sync-records-trigger"
                    render={
                        <Button
                            variant="ghost"
                            className={listWorkspaceStyles.quietButton}
                        />
                    }
                >
                    <HistoryIcon data-icon="inline-start" aria-hidden="true" />
                    同步记录
                </SheetTrigger>
                <PermissionGuard
                    permission={"supplier_api_connection:catalog_sync"}
                >
                    <LoadingButton
                        id="supplier-api-connections-catalog-sync"
                        loading={syncing}
                        disabled={!canSync || syncing}
                        title={syncBlocker}
                        onClick={() => void onSync().catch(() => undefined)}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {syncLabel}
                    </LoadingButton>
                </PermissionGuard>
            </>
        ),
    }
    return (
        <>
            <SheetContent
                size="detail"
                closeButtonId="supplier-catalog-sync-records-close"
            >
                <SheetHeader>
                    <SheetTitle>同步记录</SheetTitle>
                    <SheetDescription>{conn.supplier.name}</SheetDescription>
                </SheetHeader>
                <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6 break-words">
                    {query.isPending ? (
                        <p
                            role="status"
                            className="py-8 text-center text-xs text-muted-foreground"
                        >
                            正在读取同步记录…
                        </p>
                    ) : null}
                    {query.isError ? (
                        <BusinessFailureState
                            title="同步记录读取失败"
                            description={`${query.error.message}。请点击下方刷新重试。`}
                        />
                    ) : null}
                    {query.data ? (
                        <CatalogSyncRecords data={query.data} />
                    ) : null}
                    {syncBlocker ? (
                        <PermissionGuard
                            permission={"supplier_api_connection:catalog_sync"}
                        >
                            <p className="mt-4 text-xs text-muted-foreground">
                                {syncBlocker}
                            </p>
                        </PermissionGuard>
                    ) : null}
                </div>
                <SheetFooter>
                    <Button
                        id="supplier-api-connections-catalog-refresh"
                        variant="outline"
                        disabled={query.isFetching}
                        onClick={() =>
                            void queryClient.invalidateQueries({
                                queryKey: ["supplier-api-connections"],
                            })
                        }
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        刷新
                    </Button>
                    <SheetClose
                        id="supplier-catalog-sync-records-footer-close"
                        render={<Button variant="outline" />}
                    >
                        关闭
                    </SheetClose>
                </SheetFooter>
            </SheetContent>
            <ListWorkspacePage {...workspace}>
                {query.isSuccess && query.data.total === 0 ? (
                    <CatalogOnboarding
                        step={1}
                        title={
                            syncing
                                ? "正在同步供应商商品"
                                : syncInterrupted
                                  ? "商品同步尚未完成"
                                  : latest
                                    ? "暂未获取到供应商商品"
                                    : "同步商品，开始选品"
                        }
                        description={
                            syncing
                                ? "商品正在后台读取，完成后会显示在这里。你可以先处理其他工作，稍后回来选品。"
                                : syncInterrupted
                                  ? (latest?.error_summary ??
                                    "上次同步已中断，请继续同步；若仍无法完成，可检查连接或查看同步记录。")
                                  : latest
                                    ? "本次同步未获取到商品，请检查供应商授权的商品范围与同步记录，再重新同步。"
                                    : `已选择${conn.supplier.name}。同步供应商授权的商品目录后，即可查看商品并选择需要经营的商品入库。`
                        }
                        action={
                            <>
                                <div className="flex flex-wrap justify-center gap-2">
                                    <PermissionGuard
                                        permission={
                                            "supplier_api_connection:catalog_sync"
                                        }
                                    >
                                        <LoadingButton
                                            id="supplier-catalog-empty-sync"
                                            variant="secondary"
                                            size="sm"
                                            loading={syncing}
                                            disabled={!canSync || syncing}
                                            aria-describedby={
                                                syncBlocker
                                                    ? "supplier-catalog-empty-sync-blocker"
                                                    : undefined
                                            }
                                            onClick={() =>
                                                void onSync().catch(
                                                    () => undefined,
                                                )
                                            }
                                        >
                                            {syncLabel}
                                        </LoadingButton>
                                    </PermissionGuard>
                                    {!syncing ? (
                                        <Button
                                            id="supplier-catalog-empty-settings"
                                            variant="ghost"
                                            size="sm"
                                            className={
                                                listWorkspaceStyles.quietButton
                                            }
                                            render={
                                                <Link href={settingsHref} />
                                            }
                                        >
                                            前往平台设置
                                        </Button>
                                    ) : null}
                                </div>
                                {syncBlocker ? (
                                    <PermissionGuard
                                        permission={
                                            "supplier_api_connection:catalog_sync"
                                        }
                                    >
                                        <p
                                            id="supplier-catalog-empty-sync-blocker"
                                            className="text-xs text-muted-foreground"
                                        >
                                            {syncBlocker}
                                        </p>
                                    </PermissionGuard>
                                ) : null}
                            </>
                        }
                    />
                ) : (
                    <CatalogBrowser
                        title={workspace.title}
                        connectionId={conn.connectionId}
                        canAdopt={canAdopt}
                        adoptionBlocker={adoptionBlocker}
                        settingsHref={settingsHref}
                        canReadCost={canReadCost}
                        canReview={canReview}
                    />
                )}
            </ListWorkspacePage>
        </>
    )
}
