"use client"

import * as React from "react"
import Link from "next/link"
import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    OptionCombobox,
    PageScaffold,
} from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    useCatalogQuery,
    useConnectionCenterQuery,
    useConnectionListQuery,
    useStartCatalogSyncMutation,
} from "../hooks/queries"
import { catalogRunning } from "../api/catalog"
import { newIdempotencyKey, outcomeToResult } from "../lib/operations"
import { CatalogSection } from "../components/sections/catalog-section"
import type { ResultState } from "@/components/business/feedback"

export function SupplierCatalogPage() {
    const router = useRouter()
    const params = useSearchParams()
    const connectionId = params.get("connectionId") ?? ""
    const [search, setSearch] = React.useState("")
    const connections = useConnectionListQuery({
        q: search || undefined,
        page: 1,
        pageSize: 100,
    })
    const center = useConnectionCenterQuery(connectionId || undefined)
    const catalog = useCatalogQuery(connectionId, 1)
    const sync = useStartCatalogSyncMutation()
    const [result, setResult] = React.useState<ResultState>(null)
    const selectConnection = React.useCallback(
        (id: string) => {
            const next = new URLSearchParams()
            next.set("connectionId", id)
            router.replace(`/supplier-api/catalog?${next.toString()}`, {
                scroll: false,
            })
            setResult(null)
        },
        [router],
    )
    React.useEffect(() => {
        const first = connections.data?.items[0]
        if (!connectionId && first) selectConnection(first.connectionId)
    }, [connectionId, connections.data?.items, selectConnection])
    const conn = center.data
    const options = (connections.data?.items ?? []).map((item) => ({
        value: item.connectionId,
        label: `${item.supplier.name} · ${item.connectionCode}`,
    }))
    if (conn && !options.some((option) => option.value === conn.connectionId))
        options.unshift({
            value: conn.connectionId,
            label: `${conn.supplier.name} · ${conn.connectionCode}`,
        })
    return (
        <PageScaffold density="compact" className="gap-5">
            <ListWorkspaceHeader
                eyebrow="采购选品"
                title="供应商品库"
                description="同步供应商商品，选择需要经营的商品纳入公司商品库。"
            >
                <Button
                    id="supplier-catalog-connections-settings"
                    variant="outline"
                    render={<Link href="/supplier-api/connections" />}
                >
                    管理接口连接
                </Button>
            </ListWorkspaceHeader>
            <div className="max-w-xl space-y-2">
                <label
                    htmlFor="supplier-catalog-connection"
                    className="text-sm font-medium"
                >
                    供应商
                </label>
                <OptionCombobox
                    id="supplier-catalog-connection"
                    value={connectionId || undefined}
                    options={options}
                    onValueChange={(id) => {
                        if (id) selectConnection(id)
                    }}
                    onSearchChange={setSearch}
                    filterMode="remote"
                    retainMissingSelection={false}
                    loading={connections.isFetching}
                    onRefresh={() =>
                        connections.refetch({ throwOnError: true })
                    }
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
            {result && result.status !== "processing" ? (
                <FormalActionResult
                    status={
                        result.status === "failed" ? "rejected" : result.status
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                />
            ) : null}
            {sync.error ? (
                <p role="alert" className="text-sm text-destructive">
                    {sync.error.message}
                </p>
            ) : null}
            {!connectionId && connections.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在读取供应商…
                </p>
            ) : null}
            {!connectionId && !connections.isPending && !connections.isError ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="尚无供应商 API 连接"
                    description="先在已有供应商档案基础上配置 API 连接，再同步商品选品。"
                />
            ) : null}
            {connectionId && center.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在读取供应商商品资料…
                </p>
            ) : null}
            {center.isError ? (
                <BusinessFailureState
                    title="供应商连接读取失败"
                    description={center.error.message}
                    onRetry={() => void center.refetch()}
                />
            ) : null}
            {conn ? (
                <CatalogSection
                    key={conn.connectionId}
                    conn={conn}
                    query={catalog}
                    syncing={
                        sync.isPending || catalogRunning(catalog.data?.jobs[0])
                    }
                    canSync={
                        conn.allowedActions.includes("START_CATALOG_SYNC") &&
                        !catalogRunning(catalog.data?.jobs[0])
                    }
                    blocker={
                        conn.actionBlockers.find(
                            (blocker) =>
                                blocker.action === "START_CATALOG_SYNC",
                        )?.message
                    }
                    onSettings={() =>
                        router.push(
                            `/supplier-api/connections/${encodeURIComponent(conn.connectionId)}?section=settings`,
                        )
                    }
                    onSync={async () => {
                        const outcome = await sync.mutateAsync({
                            connectionId: conn.connectionId,
                            expectedVersion: conn.version,
                            idempotencyKey: newIdempotencyKey("catalog"),
                        })
                        setResult(outcomeToResult(outcome))
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
