"use client"

import * as React from "react"
import Link from "next/link"
import { PlugZapIcon } from "lucide-react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    OptionCombobox,
    PageScaffold,
} from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Sheet, SheetTrigger } from "@/components/ui/sheet"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    useCatalogQuery,
    useConnectionCenterQuery,
    useConnectionListQuery,
    useStartCatalogSyncMutation,
} from "../hooks/queries"
import { catalogRunning } from "../api/catalog"
import { newIdempotencyKey, outcomeToResult } from "../lib/operations"
import { ConnectionControls } from "../components/connection-controls"
import { CatalogSection } from "../components/sections/catalog-section"
import { CatalogOnboarding } from "../components/catalog/catalog-onboarding"
import type { ConnectionCenterView } from "../types"
import type { ResultState } from "@/components/business/feedback"

export function SupplierCatalogPage() {
    const router = useRouter()
    const params = useSearchParams()
    const connectionId = params.get("connectionId") ?? ""
    const [search, setSearch] = React.useState("")
    const [recordsConnectionId, setRecordsConnectionId] = React.useState<
        string | null
    >(null)
    const connections = useConnectionListQuery({
        q: search || undefined,
        page: 1,
        pageSize: 100,
    })
    const center = useConnectionCenterQuery(connectionId || undefined)
    const selectConnection = React.useCallback(
        (id: string) => {
            const next = new URLSearchParams()
            next.set("connectionId", id)
            router.replace(`/supplier-api/catalog?${next.toString()}`, {
                scroll: false,
            })
            setSearch("")
            setRecordsConnectionId(null)
        },
        [router],
    )
    React.useEffect(() => {
        const first = connections.data?.items[0]
        if (!connectionId && !search && first)
            selectConnection(first.connectionId)
    }, [connectionId, connections.data?.items, search, selectConnection])
    const conn = center.data
    const missingConnection = Boolean(connectionId) && center.isSuccess && !conn
    const noConnections =
        !connectionId &&
        !search &&
        connections.isSuccess &&
        connections.data.items.length === 0
    const options = (connections.data?.items ?? []).map((item) => ({
        value: item.connectionId,
        label: `${item.supplier.name} · ${item.connectionCode}`,
    }))
    if (conn && !options.some((option) => option.value === conn.connectionId))
        options.unshift({
            value: conn.connectionId,
            label: `${conn.supplier.name} · ${conn.connectionCode}`,
        })
    const supplierFilter = (
        <div className="w-full space-y-2 sm:max-w-sm">
            <label
                htmlFor="supplier-catalog-connection"
                className="text-sm font-medium"
            >
                供应商筛选
            </label>
            <OptionCombobox
                id="supplier-catalog-connection"
                value={
                    missingConnection ? undefined : connectionId || undefined
                }
                options={options}
                onValueChange={(id) => {
                    if (id) selectConnection(id)
                }}
                onSearchChange={setSearch}
                searchPlaceholder="搜索供应商名称或接口编号"
                filterMode="remote"
                retainMissingSelection={false}
                loading={connections.isFetching}
                onRefresh={() => connections.refetch({ throwOnError: true })}
                allowClear={false}
                placeholder="搜索并选择供应商"
                emptyLabel={
                    connections.isError
                        ? "供应商读取失败，请重试"
                        : "没有匹配的供应商连接"
                }
            />
            {connections.isError ? (
                <p role="alert" className="text-sm text-destructive">
                    {connections.error.message}
                </p>
            ) : null}
        </div>
    )
    return (
        <Sheet
            open={Boolean(conn && recordsConnectionId === conn.connectionId)}
            onOpenChange={(open) =>
                setRecordsConnectionId(open && conn ? conn.connectionId : null)
            }
        >
            <PageScaffold density="compact" className="gap-5">
                <ListWorkspaceHeader
                    eyebrow="采购选品"
                    title="供应商品库"
                    description="同步供应商商品，选择需要经营的商品纳入公司商品库。"
                >
                    {conn ? (
                        <SheetTrigger
                            id="supplier-catalog-sync-records-trigger"
                            render={<Button variant="outline" />}
                        >
                            同步记录
                        </SheetTrigger>
                    ) : null}
                    <Button
                        id="supplier-catalog-connections-settings"
                        variant="outline"
                        render={<Link href="/supplier-api/connections" />}
                    >
                        关联供应商
                    </Button>
                </ListWorkspaceHeader>
                {!conn && !noConnections ? supplierFilter : null}
                {!connectionId && connections.isPending ? (
                    <p role="status" className="text-sm text-muted-foreground">
                        正在读取供应商…
                    </p>
                ) : null}
                {noConnections ? (
                    <CatalogOnboarding
                        step={0}
                        title="连接供应商，开始选品"
                        description="当前还没有供应商接口连接。先关联已有供应商，配置就绪后即可同步商品并选品入库。"
                        action={
                            <Button
                                id="supplier-catalog-setup-connection"
                                render={
                                    <Link href="/supplier-api/connections" />
                                }
                            >
                                <PlugZapIcon aria-hidden="true" />
                                关联供应商
                            </Button>
                        }
                    />
                ) : null}
                {!connectionId && !noConnections && connections.isSuccess ? (
                    <BusinessEmptyState
                        kind="no-data"
                        title="选择供应商，查看商品库"
                        description="在上方搜索并选择供应商，即可查看商品与同步进度。"
                        action={
                            <Button
                                id="supplier-catalog-select-connection"
                                onClick={() =>
                                    document
                                        .getElementById(
                                            "supplier-catalog-connection",
                                        )
                                        ?.focus()
                                }
                            >
                                选择供应商
                            </Button>
                        }
                    />
                ) : null}
                {!connectionId && connections.isError ? (
                    <BusinessFailureState
                        idPrefix="supplier-catalog-connections"
                        title="供应商列表读取失败"
                        description="暂时无法读取供应商，请重试后再选择。"
                        onRetry={() => void connections.refetch()}
                    />
                ) : null}
                {connectionId && center.isPending ? (
                    <p role="status" className="text-sm text-muted-foreground">
                        正在读取供应商商品资料…
                    </p>
                ) : null}
                {center.isError ? (
                    <BusinessFailureState
                        idPrefix="supplier-catalog-connection"
                        title="供应商连接读取失败"
                        description={center.error.message}
                        onRetry={() => void center.refetch()}
                    />
                ) : null}
                {missingConnection ? (
                    <CatalogOnboarding
                        step={0}
                        title="此供应商连接已不可用"
                        description="原链接对应的连接已不存在。请重新选择供应商，或前往关联供应商检查配置。"
                        action={
                            <Button
                                id="supplier-catalog-reselect-connection"
                                onClick={() => {
                                    router.replace("/supplier-api/catalog", {
                                        scroll: false,
                                    })
                                    setSearch("")
                                    setRecordsConnectionId(null)
                                }}
                            >
                                重新选择供应商
                            </Button>
                        }
                    />
                ) : null}
                {conn ? (
                    <SupplierCatalogContent
                        key={conn.connectionId}
                        conn={conn}
                        supplierFilter={supplierFilter}
                    />
                ) : null}
            </PageScaffold>
        </Sheet>
    )
}

function SupplierCatalogContent({
    conn,
    supplierFilter,
}: {
    conn: ConnectionCenterView
    supplierFilter: React.ReactNode
}) {
    const [settingsOpen, setSettingsOpen] = React.useState(false)
    const catalog = useCatalogQuery(conn.connectionId, 1)
    const sync = useStartCatalogSyncMutation()
    const [result, setResult] = React.useState<ResultState>(null)
    return (
        <>
            <ConnectionControls
                conn={conn}
                open={settingsOpen}
                onOpenChange={setSettingsOpen}
            />
            <CatalogSection
                syncRecordsInSheet
                conn={conn}
                query={catalog}
                supplierFilter={supplierFilter}
                feedback={
                    <>
                        {result && result.status !== "processing" ? (
                            <FormalActionResult
                                status={
                                    result.status === "failed"
                                        ? "rejected"
                                        : result.status
                                }
                                title={result.title}
                                description={result.description}
                                reference={result.reference}
                                facts={result.facts}
                            />
                        ) : null}
                        {sync.error ? (
                            <p
                                role="alert"
                                className="text-sm text-destructive"
                            >
                                {sync.error.message}
                            </p>
                        ) : null}
                    </>
                }
                syncing={
                    sync.isPending || catalogRunning(catalog.data?.jobs[0])
                }
                canSync={
                    conn.allowedActions.includes("START_CATALOG_SYNC") &&
                    !catalogRunning(catalog.data?.jobs[0])
                }
                blocker={
                    conn.actionBlockers.find(
                        (blocker) => blocker.action === "START_CATALOG_SYNC",
                    )?.message
                }
                onSettings={() => setSettingsOpen(true)}
                onSync={async () => {
                    setResult(null)
                    const outcome = await sync.mutateAsync({
                        connectionId: conn.connectionId,
                        expectedVersion: conn.version,
                        idempotencyKey: newIdempotencyKey("catalog"),
                    })
                    setResult(outcomeToResult(outcome))
                }}
            />
        </>
    )
}
