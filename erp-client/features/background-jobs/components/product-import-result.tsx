"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import type { BackgroundJobView } from "../api"
import { useProductImportResultsDownload } from "../queries"
import { FilteredImportResult } from "./filtered-import-result"

export function ProductImportResult({
    job,
    canReadItems,
    currentUserId,
}: {
    job: BackgroundJobView
    canReadItems: boolean
    currentUserId?: string
}) {
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canList = hasPermission(permissions, "product:list")
    const canDownload =
        canReadItems &&
        job.requested_by === currentUserId &&
        canList &&
        hasPermission(permissions, "product:create")
    const download = useProductImportResultsDownload(job.id, job.job_no)

    return (
        <FilteredImportResult
            job={job}
            canReadItems={canReadItems}
            canDownload={canDownload}
            kind="product"
            listLink={
                canList
                    ? { href: "/master-data/products", label: "前往商品列表" }
                    : undefined
            }
            download={download}
        />
    )
}
