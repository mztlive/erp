"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import * as React from "react"
import { useRouter, useSearchParams } from "@/lib/navigation"

import { CategoryDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { CategoryReviseDialog } from "@/features/master-data/components/category/category-form-dialogs"
import {
    ObjectCenterQueryState,
    ObjectCenterView,
} from "@/features/master-data/components/shared/object-center-view"
import { useMasterDataCenterQuery } from "@/features/master-data/hooks/queries"

export function CategoryObjectPage({
    stableId,
    section,
}: {
    stableId: string
    section?: string
}) {
    const router = useRouter()
    const query = useMasterDataCenterQuery("categories", stableId)
    const [reviseOpen, setReviseOpen] = React.useState(false)
    const [disableOpen, setDisableOpen] = React.useState(false)
    const returnTo = useSearchParams().get("returnTo")
    const listHref =
        returnTo === "/master-data/categories" ||
        returnTo?.startsWith("/master-data/categories?")
            ? returnTo
            : "/master-data/categories"

    if (query.isPending || query.isError || !query.data) {
        return (
            <ObjectCenterQueryState
                title="商品分类详情"
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
            idPrefix="master-data-category-object-view"
            data={query.data}
            baseHref={`/master-data/categories/${query.data.stableId}${returnTo && listHref === returnTo ? `?${new URLSearchParams({ returnTo })}` : ""}`}
            section={section}
            onBack={() => router.push(listHref)}
            onRevise={() => setReviseOpen(true)}
            onDisable={() => setDisableOpen(true)}
            dialogs={
                <>
                    <PermissionGuard permission="product_category:update">
                        <CategoryReviseDialog
                            open={reviseOpen}
                            onOpenChange={setReviseOpen}
                            target={query.data}
                        />
                    </PermissionGuard>
                    <PermissionGuard permission="product_category:update">
                        <CategoryDisableDialog
                            open={disableOpen}
                            onOpenChange={setDisableOpen}
                            target={query.data}
                        />
                    </PermissionGuard>
                </>
            }
        />
    )
}
