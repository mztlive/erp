"use client"

import type { UseQueryResult } from "@tanstack/react-query"
import { useQueryClient } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import {
    CATALOG_JOB_LABEL,
    catalogRunning,
    type CatalogResult,
} from "../../api/catalog"
import type { ConnectionCenterView } from "../../types"
import { CatalogBrowser } from "../catalog/catalog-browser"

export function CatalogSection({
    conn,
    query,
    syncing,
    canSync,
    blocker,
    onSync,
    onSettings,
}: {
    conn: ConnectionCenterView
    query: UseQueryResult<CatalogResult, Error>
    syncing: boolean
    canSync: boolean
    blocker?: string
    onSync: () => Promise<void>
    onSettings: () => void
}) {
    const account = useAccountProfileQuery()
    const queryClient = useQueryClient()
    const latest = query.data?.jobs[0]
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
    return (
        <div className="space-y-5">
            <section
                className="flex flex-wrap items-start justify-between gap-4 rounded-xl border bg-card p-4"
                aria-label="商品同步状态"
            >
                <div className="min-w-0 space-y-1.5">
                    <p className="text-sm font-medium">
                        {conn.supplier.name} · 商品同步
                    </p>
                    <p className="text-sm text-muted-foreground">
                        {latest
                            ? `${latest.stalled ? "同步已中断，可继续" : CATALOG_JOB_LABEL[latest.status]} · 已读取 ${latest.observed_count} 条规格 · ${latest.pages} 页`
                            : "尚未同步供应商商品"}
                    </p>
                    {query.data ? (
                        <p className="text-xs text-muted-foreground">
                            {query.data.incremental_supported
                                ? "支持增量更新"
                                : "采用全量读取校对"}{" "}
                            ·{" "}
                            {query.data.coverage === "COMPLETE"
                                ? "覆盖供应商授权的完整目录"
                                : "当前来源仅提供部分目录，未返回不能视为下架"}
                        </p>
                    ) : null}
                    {latest?.error_summary ? (
                        <p className="text-sm text-destructive">
                            {latest.error_summary}
                        </p>
                    ) : null}
                    {!canSync && !syncing ? (
                        <p className="text-xs text-muted-foreground">
                            {blocker ??
                                "当前账号没有商品同步权限，请联系管理员。"}
                        </p>
                    ) : null}
                    {catalogRunning(latest) ? (
                        <p className="text-xs text-muted-foreground">
                            后台同步中，可离开页面后再查看进度。
                        </p>
                    ) : null}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id="supplier-api-connections-catalog-refresh"
                        variant="ghost"
                        disabled={query.isFetching}
                        onClick={() =>
                            void queryClient.invalidateQueries({
                                queryKey: [
                                    "supplier-api-connections",
                                    "catalog",
                                    conn.connectionId,
                                ],
                            })
                        }
                    >
                        刷新
                    </Button>
                    <Button
                        id="supplier-api-connections-catalog-settings"
                        variant="outline"
                        onClick={onSettings}
                    >
                        接口设置
                    </Button>
                    <LoadingButton
                        id="supplier-api-connections-catalog-sync"
                        loading={syncing}
                        disabled={!canSync || syncing}
                        onClick={() => void onSync().catch(() => undefined)}
                    >
                        {syncing
                            ? "正在同步…"
                            : latest?.stalled || latest?.status === "failed"
                              ? "继续同步商品"
                              : latest
                                ? "同步最新商品"
                                : "开始同步商品"}
                    </LoadingButton>
                </div>
            </section>
            {query.isError ? (
                <p role="alert" className="text-sm text-destructive">
                    同步状态读取失败：{query.error.message}
                </p>
            ) : null}
            <CatalogBrowser
                key={conn.connectionId}
                connectionId={conn.connectionId}
                supplierId={conn.supplier.id}
                canAdopt={canAdopt}
                canReadCost={canReadCost}
                canReview={canReview}
            />
        </div>
    )
}
