"use client"

import * as React from "react"
import Link from "next/link"
import { PlugZapIcon } from "lucide-react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    ListWorkspacePage,
    listWorkspaceEmptyStateClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Sheet } from "@/components/ui/sheet"
import { useSearchParams } from "@/lib/navigation"
import {
    useCatalogQuery,
    useConnectionCenterQuery,
    useStartCatalogSyncMutation,
} from "../hooks/queries"
import { catalogRunning } from "../api/catalog"
import { newIdempotencyKey, outcomeToResult } from "../lib/operations"
import { buildSupplierConnectionSettingsHref } from "../lib/url-state"
import { CatalogSection } from "../components/sections/catalog-section"
import type { ConnectionCenterView } from "../types"
import type { ResultState } from "@/components/business/feedback"

export function SupplierCatalogPage() {
    const params = useSearchParams()
    const connectionId = params.get("connectionId") ?? ""
    const [recordsConnectionId, setRecordsConnectionId] = React.useState<
        string | null
    >(null)
    const center = useConnectionCenterQuery(connectionId || undefined)
    const conn = center.data
    const connectionFailure = center.isError ? (
        <BusinessFailureState
            idPrefix="supplier-catalog-connection"
            title="供应商连接读取失败"
            description={center.error.message}
            onRetry={() => void center.refetch()}
        />
    ) : null

    if (!connectionId) {
        return (
            <ListWorkspacePage title="供应商品库">
                <BusinessEmptyState
                    kind="no-data"
                    icon={PlugZapIcon}
                    className={listWorkspaceEmptyStateClassName}
                    title="请选择供应商连接"
                    description="供应商品库按供应商连接隔离。先在「平台连接」选择供应商，再进入商品库同步商品。"
                    action={
                        <Button
                            id="supplier-catalog-open-connections"
                            variant="secondary"
                            size="sm"
                            render={<Link href="/supplier-api/connections" />}
                        >
                            <PlugZapIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            前往平台连接
                        </Button>
                    }
                />
            </ListWorkspacePage>
        )
    }
    if (!conn) {
        return (
            <ListWorkspacePage title="供应商品库" loading={center.isPending}>
                {connectionFailure ??
                    (center.isSuccess ? (
                        <BusinessEmptyState
                            kind="no-data"
                            icon={PlugZapIcon}
                            className={listWorkspaceEmptyStateClassName}
                            title="此供应商连接已不可用"
                            description="原链接对应的连接已不存在。请回到平台连接重新选择供应商。"
                            action={
                                <Button
                                    id="supplier-catalog-reselect-connection"
                                    variant="secondary"
                                    size="sm"
                                    render={
                                        <Link href="/supplier-api/connections" />
                                    }
                                >
                                    返回平台连接
                                </Button>
                            }
                        />
                    ) : null)}
            </ListWorkspacePage>
        )
    }
    return (
        <Sheet
            open={recordsConnectionId === conn.connectionId}
            onOpenChange={(open) =>
                setRecordsConnectionId(open ? conn.connectionId : null)
            }
        >
            <SupplierCatalogContent
                key={conn.connectionId}
                conn={conn}
                connectionFailure={connectionFailure}
            />
        </Sheet>
    )
}

function SupplierCatalogContent({
    conn,
    connectionFailure,
}: {
    conn: ConnectionCenterView
    connectionFailure: React.ReactNode
}) {
    const catalog = useCatalogQuery(conn.connectionId, 1)
    const sync = useStartCatalogSyncMutation()
    const [result, setResult] = React.useState<ResultState>(null)
    return (
        <CatalogSection
            conn={conn}
            query={catalog}
            settingsHref={buildSupplierConnectionSettingsHref(
                conn.connectionId,
            )}
            feedback={
                <>
                    {connectionFailure}
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
                        <BusinessFailureState
                            title="商品同步未启动"
                            description={sync.error.message}
                        />
                    ) : null}
                </>
            }
            syncing={sync.isPending || catalogRunning(catalog.data?.jobs[0])}
            canSync={
                conn.allowedActions.includes("START_CATALOG_SYNC") &&
                !catalogRunning(catalog.data?.jobs[0])
            }
            blocker={
                conn.actionBlockers.find(
                    (blocker) => blocker.action === "START_CATALOG_SYNC",
                )?.message
            }
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
    )
}
