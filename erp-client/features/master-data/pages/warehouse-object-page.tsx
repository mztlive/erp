"use client"

import * as React from "react"
import { Settings2Icon } from "lucide-react"
import { useRouter, useSearchParams } from "@/lib/navigation"

import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { WarehouseReviseDialog } from "@/features/master-data/components/warehouse/warehouse-action-dialogs"
import {
    ObjectCenterQueryState,
    ObjectCenterView,
} from "@/features/master-data/components/shared/object-center-view"
import { useMasterDataCenterQuery } from "@/features/master-data/hooks/queries"

export function WarehouseObjectPage({
    stableId,
    section,
}: {
    stableId: string
    section?: string
}) {
    const router = useRouter()
    const query = useMasterDataCenterQuery("warehouses", stableId)
    const [reviseOpen, setReviseOpen] = React.useState(false)
    const returnTo = useSearchParams().get("returnTo")
    const hasValidReturnTo =
        returnTo === "/master-data/warehouses" ||
        Boolean(returnTo?.startsWith("/master-data/warehouses?"))
    const listHref =
        hasValidReturnTo && returnTo ? returnTo : "/master-data/warehouses"

    if (query.isPending || query.isError || !query.data) {
        return (
            <ObjectCenterQueryState
                idPrefix="master-data-warehouse-object-view"
                title="仓库详情"
                listHref={listHref}
                backVisibility={hasValidReturnTo ? "always" : undefined}
                isPending={query.isPending}
                isError={query.isError}
                error={query.error}
                onRetry={() => void query.refetch()}
                missing={!query.isPending && !query.isError && !query.data}
            />
        )
    }

    const baseHref = `/master-data/warehouses/${query.data.stableId}${hasValidReturnTo && returnTo ? `?${new URLSearchParams({ returnTo })}` : ""}`

    return (
        <ObjectCenterView
            idPrefix="master-data-warehouse-object-view"
            data={query.data}
            baseHref={baseHref}
            section={section}
            backVisibility={hasValidReturnTo ? "always" : undefined}
            primaryActionConfig={{
                action: "MAINTAIN_FULFILLMENT_HANDLERS",
                permission: "warehouse:update",
                label: "配置收发责任",
                icon: Settings2Icon,
                blockedMessage: (message) => `暂时不能配置收发责任：${message}`,
            }}
            hideDisableAction
            onBack={() => router.push(listHref)}
            onRevise={() => setReviseOpen(true)}
            dialogs={
                <PermissionGuard permission="warehouse:update">
                    <WarehouseReviseDialog
                        open={reviseOpen}
                        onOpenChange={setReviseOpen}
                        target={query.data}
                    />
                </PermissionGuard>
            }
        />
    )
}
