"use client"

import type { ReactNode } from "react"
import type { UseQueryResult } from "@tanstack/react-query"
import { useQueryClient } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    SheetClose,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import {
    CATALOG_JOB_LABEL,
    catalogRunning,
    type CatalogResult,
} from "../../api/catalog"
import type { ConnectionCenterView } from "../../types"
import { CatalogBrowser } from "../catalog/catalog-browser"
import { CatalogOnboarding } from "../catalog/catalog-onboarding"

export function CatalogSection({
    conn,
    query,
    syncing,
    canSync,
    blocker,
    onSync,
    onSettings,
    supplierFilter,
    feedback,
    syncRecordsInSheet = false,
}: {
    conn: ConnectionCenterView
    supplierFilter?: ReactNode
    feedback?: ReactNode
    syncRecordsInSheet?: boolean
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
    const syncRecords = (
        <>
            <section
                className="flex flex-wrap items-start justify-between gap-4 pt-4"
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
                    {syncBlocker ? (
                        <p className="text-xs text-muted-foreground">
                            {syncBlocker}
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
                                queryKey: ["supplier-api-connections"],
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
                        {syncLabel}
                    </LoadingButton>
                </div>
            </section>
            {!syncRecordsInSheet ? feedback : null}
            {query.isError ? (
                <p role="alert" className="text-sm text-destructive">
                    同步状态读取失败：{query.error.message}
                </p>
            ) : null}
            <div
                className={
                    syncRecordsInSheet ? undefined : "max-h-80 overflow-y-auto"
                }
            >
                {query.data?.jobs.map((job) => (
                    <div
                        key={job.job_id}
                        className="mt-3 space-y-1 border-t pt-3 text-sm"
                    >
                        <p>
                            {job.job_no} · {CATALOG_JOB_LABEL[job.status]} ·
                            已读取 {job.observed_count} 条规格 · 跳过{" "}
                            {job.skipped_count ?? 0} 项
                        </p>
                        {job.error_summary ? (
                            <p className="text-destructive">
                                {job.error_summary}
                            </p>
                        ) : null}
                        {job.skipped_count ? (
                            <p className="text-muted-foreground">
                                以下为最近 {job.skipped_samples?.length ?? 0}{" "}
                                条异常；修正来源数据后重新同步可补齐。
                            </p>
                        ) : null}
                        <ul className="space-y-1 text-muted-foreground">
                            {job.skipped_samples?.map((message, index) => (
                                <li key={`${job.job_id}-${index}`}>
                                    {message}
                                </li>
                            ))}
                        </ul>
                    </div>
                ))}
            </div>
        </>
    )
    return (
        <div className="space-y-5">
            {syncRecordsInSheet ? (
                <SheetContent
                    size="detail"
                    closeButtonId="supplier-catalog-sync-records-close"
                >
                    <SheetHeader>
                        <SheetTitle>同步记录</SheetTitle>
                        <SheetDescription>
                            {conn.supplier.name}
                        </SheetDescription>
                    </SheetHeader>
                    <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6 break-words">
                        {syncRecords}
                    </div>
                    <SheetFooter>
                        <SheetClose
                            id="supplier-catalog-sync-records-footer-close"
                            render={<Button variant="outline" />}
                        >
                            关闭
                        </SheetClose>
                    </SheetFooter>
                </SheetContent>
            ) : (
                <details className="rounded-xl border bg-card p-4">
                    <summary
                        id="supplier-catalog-sync-records"
                        className="cursor-pointer text-sm font-medium"
                    >
                        {conn.supplier.name} · 同步记录
                        <span className="ml-3 text-muted-foreground">
                            {latest
                                ? `${latest.stalled ? "同步已中断" : CATALOG_JOB_LABEL[latest.status]}${latest.skipped_count ? `，已跳过 ${latest.skipped_count} 项异常` : ""}`
                                : "尚未同步"}
                        </span>
                    </summary>
                    {syncRecords}
                </details>
            )}
            {syncRecordsInSheet ? feedback : null}
            {query.isSuccess && query.data.total === 0 ? (
                <div className="space-y-5">
                    {supplierFilter}
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
                                    "上次同步已中断，请继续同步；若仍无法完成，可检查接口设置或查看同步记录。")
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
                                            void onSync().catch(() => undefined)
                                        }
                                    >
                                        {syncLabel}
                                    </LoadingButton>
                                    {!syncing ? (
                                        <Button
                                            id="supplier-catalog-empty-settings"
                                            variant="outline"
                                            onClick={onSettings}
                                        >
                                            检查接口设置
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
                    supplierFilter={supplierFilter}
                    key={conn.connectionId}
                    connectionId={conn.connectionId}
                    supplierId={conn.supplier.id}
                    canAdopt={canAdopt}
                    canReadCost={canReadCost}
                    canReview={canReview}
                />
            )}
        </div>
    )
}
