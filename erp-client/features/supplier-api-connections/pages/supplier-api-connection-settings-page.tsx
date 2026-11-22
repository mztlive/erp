"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { isNotFoundError } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"

import { ConnectionSettingsPanel } from "../components/connection-settings-panel"
import { useConnectionCenterQuery } from "../hooks/queries"
import { buildSupplierCatalogHref } from "../lib/url-state"

export function SupplierApiConnectionSettingsPage() {
    const { connectionId } = useParams<{ connectionId: string }>()
    const profile = useAccountProfileQuery()
    const canDetail = hasPermission(
        profile.data?.permissions,
        "supplier_api_connection:detail",
    )
    const center = useConnectionCenterQuery(
        connectionId,
        !profile.isPending && canDetail,
    )
    const conn = center.data
    const listBack = {
        id: `supplier-api-connection-${toAutomationIdSegment(connectionId || "list")}-back`,
        label: "返回平台连接",
        href: "/supplier-api/connections",
    }

    if (profile.isPending || (canDetail && center.isPending)) {
        return (
            <DetailWorkspaceSkeleton
                title="平台连接设置"
                description="正在读取连接状态与接口配置…"
            />
        )
    }

    if (profile.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="平台连接设置" back={listBack} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            idPrefix="supplier-api-connection-settings-profile"
                            title="账号权限读取失败"
                            error={profile.error}
                            onRetry={() => void profile.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!canDetail) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="平台连接设置" back={listBack} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="permission"
                            title="权限不足"
                            description="当前账号没有查看平台连接设置的权限，请联系管理员开通。"
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (center.isError) {
        const notFound = isNotFoundError(center.error)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="平台连接设置" back={listBack} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        {notFound ? (
                            <BusinessEmptyState
                                kind="no-data"
                                title="未找到平台连接"
                                description="该连接不存在或已移除，请返回平台连接列表核对。"
                                action={
                                    <Button
                                        id="supplier-api-connection-settings-back-to-list"
                                        variant="outline"
                                        size="sm"
                                        render={
                                            <Link href="/supplier-api/connections" />
                                        }
                                    >
                                        返回平台连接
                                    </Button>
                                }
                            />
                        ) : (
                            <BusinessFailureState
                                idPrefix="supplier-api-connection-settings"
                                title="连接加载失败"
                                error={center.error}
                                onRetry={() => void center.refetch()}
                            />
                        )}
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!conn) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="平台连接设置" back={listBack} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到连接"
                            description="该连接不存在或当前角色无权查看。"
                            action={
                                <Button
                                    id="supplier-api-connection-settings-back-to-list"
                                    variant="outline"
                                    size="sm"
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
                    label: "返回平台连接",
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
                secondaryActions={
                    <Button
                        id={`supplier-api-connection-${segment}-catalog-nav`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        render={
                            <Link
                                href={buildSupplierCatalogHref(
                                    conn.connectionId,
                                )}
                            />
                        }
                    >
                        打开商品库
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                }
            />
            <ConnectionSettingsPanel key={conn.connectionId} conn={conn} />
        </DetailWorkspacePage>
    )
}
