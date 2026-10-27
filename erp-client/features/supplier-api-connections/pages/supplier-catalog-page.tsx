"use client"

import * as React from "react"
import Link from "next/link"
import { PlugZapIcon } from "lucide-react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    PageScaffold,
} from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
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
    const missingConnection = Boolean(connectionId) && center.isSuccess && !conn
    if (!connectionId) {
        return (
            <PageScaffold density="compact" className="gap-5">
                <ListWorkspaceHeader
                    eyebrow="采购选品"
                    title="供应商品库"
                    description="从 API 连接选择供应商后，同步并选择需要经营的商品。"
                />
                <BusinessEmptyState
                    kind="no-data"
                    icon={PlugZapIcon}
                    title="请选择供应商连接"
                    description="供应商品库按供应商连接隔离。先在「API 连接」选择供应商，再进入商品库同步商品。"
                    action={
                        <Button
                            id="supplier-catalog-open-connections"
                            render={<Link href="/supplier-api/connections" />}
                        >
                            <PlugZapIcon aria-hidden="true" />
                            前往 API 连接
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }
    return (
        <Sheet
            open={Boolean(conn && recordsConnectionId === conn.connectionId)}
            onOpenChange={(open) =>
                setRecordsConnectionId(open && conn ? conn.connectionId : null)
            }
        >
            <PageScaffold density="compact" className="gap-5">
                {!conn ? (
                    <ListWorkspaceHeader
                        eyebrow="采购选品"
                        title="供应商品库"
                        description="同步供应商商品，选择需要经营的商品纳入公司商品库。"
                    />
                ) : null}
                {center.isPending ? (
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
                    <BusinessEmptyState
                        kind="no-data"
                        icon={PlugZapIcon}
                        title="此供应商连接已不可用"
                        description="原链接对应的连接已不存在。请回到 API 连接重新选择供应商。"
                        action={
                            <Button
                                id="supplier-catalog-reselect-connection"
                                render={
                                    <Link href="/supplier-api/connections" />
                                }
                            >
                                返回 API 连接
                            </Button>
                        }
                    />
                ) : null}
                {conn ? (
                    <SupplierCatalogContent
                        key={conn.connectionId}
                        conn={conn}
                    />
                ) : null}
            </PageScaffold>
        </Sheet>
    )
}

function SupplierCatalogContent({ conn }: { conn: ConnectionCenterView }) {
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
                        <p role="alert" className="text-sm text-destructive">
                            {sync.error.message}
                        </p>
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
