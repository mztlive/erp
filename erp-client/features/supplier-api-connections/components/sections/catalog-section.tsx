"use client"

import type { ReactNode } from "react"
import type { UseQueryResult } from "@tanstack/react-query"
import { useQueryClient } from "@tanstack/react-query"
import Link from "next/link"
import { HistoryIcon, RefreshCwIcon } from "lucide-react"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
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
import { ConnectionPageNav } from "../connection-page-nav"
import { CatalogBrowser } from "../catalog/catalog-browser"
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
        !adoptionBlocker &&
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
        <>
            <ListWorkspaceHeader
                eyebrow="采购选品"
                title={`${conn.supplier.name} · 商品库`}
                description="同步供应商商品，选择需要经营的商品纳入公司商品库。"
            >
                <SheetTrigger
                    id="supplier-catalog-sync-records-trigger"
                    render={<Button variant="outline" />}
                >
                    <HistoryIcon data-icon="inline-start" aria-hidden="true" />
                    同步记录
                </SheetTrigger>
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
            </ListWorkspaceHeader>
            <ConnectionPageNav
                connectionId={conn.connectionId}
                supplierName={conn.supplier.name}
                active="catalog"
            />
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
                            className="py-8 text-center text-sm text-muted-foreground"
                        >
                            正在读取同步记录…
                        </p>
                    ) : null}
                    {query.isError ? (
                        <p
                            role="alert"
                            className="mb-4 text-sm text-destructive"
                        >
                            同步记录读取失败：{query.error.message}
                            。请点击下方刷新重试。
                        </p>
                    ) : null}
                    {query.data ? (
                        <CatalogSyncRecords data={query.data} />
                    ) : null}
                    {syncBlocker ? (
                        <p className="mt-4 text-xs text-muted-foreground">
                            {syncBlocker}
                        </p>
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
            <div className="space-y-5">
                {feedback}
                {query.isSuccess && query.data.total === 0 ? (
                    <div className="space-y-5">
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
                                        <LoadingButton
                                            id="supplier-catalog-empty-sync"
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
                                        {!syncing ? (
                                            <Button
                                                id="supplier-catalog-empty-settings"
                                                variant="outline"
                                                render={
                                                    <Link href={settingsHref} />
                                                }
                                            >
                                                前往 API 设置
                                            </Button>
                                        ) : null}
                                    </div>
                                    {syncBlocker ? (
                                        <p
                                            id="supplier-catalog-empty-sync-blocker"
                                            className="text-sm text-muted-foreground"
                                        >
                                            {syncBlocker}
                                        </p>
                                    ) : null}
                                </>
                            }
                        />
                    </div>
                ) : (
                    <CatalogBrowser
                        supplierName={conn.supplier.name}
                        key={conn.connectionId}
                        connectionId={conn.connectionId}
                        supplierId={conn.supplier.id}
                        canAdopt={canAdopt}
                        adoptionBlocker={adoptionBlocker}
                        settingsHref={settingsHref}
                        canReadCost={canReadCost}
                        canReview={canReview}
                    />
                )}
            </div>
        </>
    )
}
