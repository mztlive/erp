"use client"

import Link from "next/link"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailPageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useParams } from "@/lib/navigation"
import { ConnectionPageNav } from "../components/connection-page-nav"
import { ConnectionSettingsPanel } from "../components/connection-settings-panel"
import { useConnectionCenterQuery } from "../hooks/queries"
import { buildSupplierCatalogHref } from "../lib/url-state"

export function SupplierApiConnectionSettingsPage() {
    const { connectionId } = useParams<{ connectionId: string }>()
    const center = useConnectionCenterQuery(connectionId)
    const conn = center.data
    if (center.isPending) {
        return (
            <PageScaffold density="compact">
                <p role="status" className="text-sm text-muted-foreground">
                    正在读取连接…
                </p>
            </PageScaffold>
        )
    }
    if (center.isError) {
        return (
            <PageScaffold density="compact">
                <BusinessFailureState
                    idPrefix="supplier-api-connection-settings"
                    title="连接加载失败"
                    description={center.error.message}
                    onRetry={() => void center.refetch()}
                />
            </PageScaffold>
        )
    }
    if (!conn) {
        return (
            <PageScaffold density="compact">
                <BusinessEmptyState
                    kind="no-data"
                    title="未找到连接"
                    description="该连接不存在或当前角色无权查看。"
                    action={
                        <Button
                            id="supplier-api-connection-settings-back-to-list"
                            render={<Link href="/supplier-api/connections" />}
                        >
                            返回 API 连接
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }
    const segment = toAutomationIdSegment(conn.connectionId)
    return (
        <PageScaffold density="compact" className="gap-5">
            <DetailPageHeader
                back={{
                    id: `supplier-api-connection-${segment}-back`,
                    label: "API 连接",
                    href: "/supplier-api/connections",
                }}
                title={conn.supplier.name}
                documentNumber={conn.connectionCode}
                numberLabel="连接代码"
                primaryStatus={{
                    label: conn.statusLabel,
                    tone: conn.statusTone,
                }}
                meta={[conn.connectorTypeLabel, conn.deploymentMessage]
                    .filter(Boolean)
                    .join(" · ")}
                primaryAction={
                    <Button
                        id={`supplier-api-connection-${segment}-open-catalog`}
                        render={
                            <Link
                                href={buildSupplierCatalogHref(
                                    conn.connectionId,
                                )}
                            />
                        }
                    >
                        进入商品库
                    </Button>
                }
            />
            <ConnectionPageNav
                connectionId={conn.connectionId}
                active="settings"
            />
            <ConnectionSettingsPanel conn={conn} />
        </PageScaffold>
    )
}
