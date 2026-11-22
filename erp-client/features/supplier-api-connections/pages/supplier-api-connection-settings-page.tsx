"use client"

import Link from "next/link"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useParams } from "@/lib/navigation"
import {
    buildSupplierCatalogHref,
    buildSupplierConnectionSettingsHref,
} from "../lib/url-state"
import { cn } from "@/lib/utils"
import { ConnectionSettingsPanel } from "../components/connection-settings-panel"
import { useConnectionCenterQuery } from "../hooks/queries"

export function SupplierApiConnectionSettingsPage() {
    const { connectionId } = useParams<{ connectionId: string }>()
    const center = useConnectionCenterQuery(connectionId)
    const conn = center.data
    if (center.isPending) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="平台设置" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在读取连接…
                        </p>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }
    if (center.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="平台设置" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            idPrefix="supplier-api-connection-settings"
                            title="连接加载失败"
                            description={center.error.message}
                            onRetry={() => void center.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }
    if (!conn) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="平台设置" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到连接"
                            description="该连接不存在或当前角色无权查看。"
                            action={
                                <Button
                                    id="supplier-api-connection-settings-back-to-list"
                                    render={
                                        <Link href="/supplier-api/connections" />
                                    }
                                >
                                    返回平台连接
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }
    const segment = toAutomationIdSegment(conn.connectionId)
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                back={{
                    id: `supplier-api-connection-${segment}-back`,
                    label: "平台连接",
                    href: "/supplier-api/connections",
                }}
                title={conn.supplier.name}
                documentNumber={conn.connectionCode}
                numberLabel="连接代码"
                primaryStatus={{
                    label: conn.statusLabel,
                    tone: conn.statusTone,
                }}
                meta={conn.connectorTypeLabel}
                tabs={
                    <nav
                        aria-label="供应商连接工作区"
                        className={detailWorkspaceStyles.tabsList}
                    >
                        <Link
                            id={`supplier-api-connection-${segment}-settings-nav`}
                            href={buildSupplierConnectionSettingsHref(
                                conn.connectionId,
                            )}
                            aria-current="page"
                            data-active="true"
                            className={cn(
                                detailWorkspaceStyles.tab,
                                "no-underline",
                            )}
                        >
                            平台设置
                        </Link>
                        <Link
                            id={`supplier-api-connection-${segment}-catalog-nav`}
                            href={buildSupplierCatalogHref(conn.connectionId)}
                            className={cn(
                                detailWorkspaceStyles.tab,
                                "no-underline",
                            )}
                        >
                            商品库
                        </Link>
                    </nav>
                }
            />
            <ConnectionSettingsPanel key={conn.connectionId} conn={conn} />
        </DetailWorkspacePage>
    )
}
