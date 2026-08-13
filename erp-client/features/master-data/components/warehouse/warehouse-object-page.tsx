"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import {
    WarehouseDisableDialog,
    WarehouseReviseDialog,
} from "@/features/master-data/components/warehouse/warehouse-action-dialogs"
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
    const [disableOpen, setDisableOpen] = React.useState(false)
    const listHref = "/master-data/warehouses"

    if (query.isPending || query.isError || !query.data) {
        return (
            <ObjectCenterQueryState
                title="仓库详情"
                listHref={listHref}
                isPending={query.isPending}
                isError={query.isError}
                error={query.error}
                onRetry={() => void query.refetch()}
                missing={!query.isPending && !query.isError && !query.data}
            />
        )
    }

    return (
        <ObjectCenterView
            data={query.data}
            listHref={listHref}
            listLabel="仓库"
            baseHref={`${listHref}/${query.data.stableId}`}
            section={section}
            onBack={() => router.push(listHref)}
            onRevise={() => setReviseOpen(true)}
            onDisable={() => setDisableOpen(true)}
            dialogs={
                <>
                    <WarehouseReviseDialog
                        open={reviseOpen}
                        onOpenChange={setReviseOpen}
                        target={query.data}
                    />
                    <WarehouseDisableDialog
                        open={disableOpen}
                        onOpenChange={setDisableOpen}
                        target={query.data}
                    />
                </>
            }
        />
    )
}
