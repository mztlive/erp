"use client"
import { DetailPageHeader } from "@/components/business"
import type { ConnectionCenterView } from "../types"
export function CenterHeader({
    conn,
    onBack,
}: {
    conn: ConnectionCenterView
    onBack: () => void
}) {
    return (
        <DetailPageHeader
            back={{
                id: "supplier-api-connections-center-back",
                label: "连接列表",
                onClick: onBack,
            }}
            title={conn.supplier.name}
            documentNumber={conn.connectionCode}
            numberLabel="连接代码"
            primaryStatus={{
                label: conn.deploymentReady ? "接口已配置" : "接口待配置",
                tone: conn.deploymentReady ? "success" : "warning",
            }}
            meta={conn.connectorTypeLabel}
        />
    )
}
