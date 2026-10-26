"use client"
import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import type { UseQueryResult } from "@tanstack/react-query"
import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Card,
    CardHeader,
    CardTitle,
    CardContent,
    CardDescription,
} from "@/components/ui/card"
import {
    CATALOG_JOB_LABEL,
    catalogRunning,
    type CatalogItem,
    type CatalogResult,
} from "../../api/catalog"
import type { ConnectionCenterView } from "../../types"
import { CatalogMatchDialog } from "../dialogs/catalog-match-dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function CatalogSection({
    conn,
    query,
    page,
    onPageChange,
    syncing,
    canSync,
    blocker,
    onSync,
    onSettings,
}: {
    conn: ConnectionCenterView
    query: UseQueryResult<CatalogResult, Error>
    page: number
    onPageChange: (page: number) => void
    syncing: boolean
    canSync: boolean
    blocker?: string
    onSync: () => Promise<void>
    onSettings: () => void
}) {
    const [selected, setSelected] = React.useState<CatalogItem | null>(null)
    const latest = query.data?.jobs[0]
    const columns = React.useMemo<ColumnDef<CatalogItem>[]>(
        () => [
            {
                accessorKey: "name",
                header: "供应商商品",
                meta: { label: "商品" },
            },
            {
                accessorKey: "specification",
                header: "规格",
                meta: { label: "规格" },
            },
            {
                accessorKey: "unit",
                header: "单位",
                meta: { label: "单位" },
                cell: ({ row }) => row.original.unit ?? "待确认",
            },
            {
                accessorKey: "purchase_price",
                header: "接口采购报价",
                meta: { label: "接口采购报价" },
                cell: ({ row }) => row.original.purchase_price ?? "未取得",
            },
            {
                id: "matching",
                header: "公司商品",
                meta: { label: "匹配情况" },
                cell: ({ row }) =>
                    row.original.sku_id
                        ? `已匹配 · ${row.original.sku_no ?? "公司规格"}`
                        : "待匹配",
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                cell: ({ row }) => (
                    <div className="flex flex-wrap gap-2">
                        {conn.allowedActions.includes("START_CATALOG_SYNC") ||
                        conn.actionBlockers.some(
                            (v) => v.action === "START_CATALOG_SYNC",
                        ) ? (
                            <Button
                                id={`supplier-catalog-match-${toAutomationIdSegment(row.original.id)}`}
                                size="sm"
                                variant="outline"
                                onClick={() => setSelected(row.original)}
                            >
                                {row.original.sku_id
                                    ? "调整匹配"
                                    : "匹配公司商品"}
                            </Button>
                        ) : null}
                        {row.original.sku_id ? (
                            <Link
                                id={`supplier-catalog-offerings-${toAutomationIdSegment(row.original.id)}`}
                                className="self-center text-sm text-primary underline underline-offset-4"
                                href={`/procurement/supplier-offerings?skuId=${encodeURIComponent(row.original.sku_id)}&supplierId=${encodeURIComponent(conn.supplier.id)}`}
                            >
                                维护供给
                            </Link>
                        ) : null}
                    </div>
                ),
            },
        ],
        [conn],
    )
    return (
        <div className="space-y-5">
            <Card>
                <CardHeader>
                    <CardTitle>同步供应商商品</CardTitle>
                    <CardDescription>
                        从{conn.supplier.name}
                        读取商品和规格，保存后在下方查看、匹配公司商品。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                    <ol className="grid gap-3 text-sm sm:grid-cols-3">
                        <li>
                            <strong>1. 接口配置</strong>
                            <p className="mt-1 text-muted-foreground">
                                {conn.deploymentReady
                                    ? "已自动读取服务端配置"
                                    : "请联系管理员完成服务端配置"}
                            </p>
                        </li>
                        <li>
                            <strong>2. 同步商品</strong>
                            <p className="mt-1 text-muted-foreground">
                                点击开始同步，后台逐页读取商品。
                            </p>
                        </li>
                        <li>
                            <strong>3. 匹配与使用</strong>
                            <p className="mt-1 text-muted-foreground">
                                匹配公司规格，再维护供给和业务使用条件。
                            </p>
                        </li>
                    </ol>
                    <div className="flex flex-wrap items-center gap-3">
                        <LoadingButton
                            id="supplier-api-connections-catalog-sync"
                            type="button"
                            loading={syncing}
                            disabled={!canSync || syncing}
                            onClick={() => void onSync().catch(() => undefined)}
                        >
                            {syncing
                                ? "正在同步…"
                                : latest?.stalled || latest?.status === "failed"
                                  ? "继续同步商品"
                                  : query.data?.total
                                    ? "重新同步商品"
                                    : "开始同步商品"}
                        </LoadingButton>
                        <Button
                            id="supplier-api-connections-catalog-settings"
                            variant="outline"
                            onClick={onSettings}
                        >
                            连接设置
                        </Button>
                        <Button
                            id="supplier-api-connections-catalog-refresh"
                            variant="ghost"
                            disabled={query.isFetching}
                            onClick={() => void query.refetch()}
                        >
                            刷新结果
                        </Button>
                    </div>
                    {!canSync && !syncing ? (
                        <p className="text-sm text-muted-foreground">
                            {blocker ??
                                "当前账号没有商品同步权限，请联系管理员。"}
                        </p>
                    ) : null}
                    {latest ? (
                        <div
                            aria-live="polite"
                            className="rounded-md bg-muted p-3 text-sm"
                        >
                            <p className="font-medium">
                                {latest.stalled
                                    ? "同步已中断，可继续同步"
                                    : CATALOG_JOB_LABEL[latest.status]}{" "}
                                · 已读取 {latest.observed_count} 条商品规格 ·{" "}
                                {latest.pages} 页
                            </p>
                            {catalogRunning(latest) ? (
                                <p className="mt-1 text-muted-foreground">
                                    可离开页面，回来后继续查看进度。
                                </p>
                            ) : null}
                            {latest.error_summary ? (
                                <p className="mt-1 text-destructive">
                                    {latest.error_summary}
                                </p>
                            ) : null}
                        </div>
                    ) : null}
                    <p className="text-xs text-muted-foreground">
                        同步内容是供应商目录。匹配不会自动建立供给、上架或改动公司销售价；本次未返回的商品会保留，不能据此判断下架。
                    </p>
                </CardContent>
            </Card>
            <BusinessTableFrame
                title="已同步商品"
                description={`共 ${query.data?.total ?? 0} 条商品规格。未取得的单位或报价需另行核实。`}
                table={
                    <DataTable
                        id="supplier-api-connections-catalog-table"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        loading={query.isPending}
                        rowCount={query.data?.total ?? 0}
                        pagination={{ pageIndex: page - 1, pageSize: 20 }}
                        onPaginationChange={(value) =>
                            onPageChange(value.pageIndex + 1)
                        }
                        caption="已同步供应商商品"
                        layout="flush"
                        errorState={
                            query.isError ? (
                                <BusinessFailureState
                                    title="商品目录加载失败"
                                    description={query.error.message}
                                    onRetry={() => void query.refetch()}
                                />
                            ) : undefined
                        }
                        emptyState={
                            <BusinessEmptyState
                                kind="no-data"
                                title={
                                    latest?.status === "succeeded"
                                        ? "本次未读取到商品"
                                        : "尚未同步商品"
                                }
                                description={
                                    latest?.status === "succeeded"
                                        ? "请核对供应商渠道的商品授权范围。"
                                        : "接口配置就绪后，点击上方“开始同步商品”。"
                                }
                            />
                        }
                    />
                }
            />
            {selected ? (
                <CatalogMatchDialog
                    key={selected.id}
                    connectionId={conn.connectionId}
                    item={selected}
                    onClose={() => setSelected(null)}
                />
            ) : null}
        </div>
    )
}
