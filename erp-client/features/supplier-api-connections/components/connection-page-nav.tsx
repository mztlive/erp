"use client"

import { ListWorkspaceViews } from "@/components/business/list-workspace"
import { toAutomationIdSegment } from "@/lib/automation-id"

import {
    buildSupplierCatalogHref,
    buildSupplierConnectionSettingsHref,
} from "../lib/url-state"

/** 一个供应商连接下的两个工作区：API 设置与商品库。 */
export function ConnectionPageNav({
    connectionId,
    supplierName,
    active,
}: {
    connectionId: string
    supplierName?: string
    active: "settings" | "catalog"
}) {
    const segment = toAutomationIdSegment(connectionId)
    return (
        <ListWorkspaceViews
            ariaLabel="供应商连接工作区"
            hint={supplierName ? `当前供应商：${supplierName}` : undefined}
            items={[
                {
                    id: `supplier-api-connection-${segment}-settings-nav`,
                    label: "API 设置",
                    href: buildSupplierConnectionSettingsHref(connectionId),
                    active: active === "settings",
                },
                {
                    id: `supplier-api-connection-${segment}-catalog-nav`,
                    label: "商品库",
                    href: buildSupplierCatalogHref(connectionId),
                    active: active === "catalog",
                },
            ]}
        />
    )
}
