"use client"

import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { fetchCustomerImportResultRows } from "@/features/customers/api/import"
import { customerKeys } from "@/features/customers/hooks/queries"
import { downloadCustomerImportResults } from "@/features/customers/lib/customer-import-workbook"
import { hasPermission } from "@/lib/permissions"
import type { BackgroundJobItemStatus, BackgroundJobView } from "../api"
import { IMPORT_RESULT_LABELS } from "../labels"
import { FilteredImportResult } from "./filtered-import-result"

export function CustomerImportResult({
    job,
    canReadItems,
    currentUserId,
}: {
    job: BackgroundJobView
    canReadItems: boolean
    currentUserId?: string
}) {
    const queryClient = useQueryClient()
    const permissions = useAccountProfileQuery().data?.permissions
    const canList = hasPermission(permissions, "customer:list")
    const canDownload =
        canReadItems &&
        job.requested_by === currentUserId &&
        hasPermission(permissions, "customer:create")
    const download = useMutation({
        mutationFn: async (status: BackgroundJobItemStatus) => {
            const rows = await fetchCustomerImportResultRows(job.id, status)
            await downloadCustomerImportResults(
                rows,
                IMPORT_RESULT_LABELS[status],
                job.job_no,
            )
        },
        retry: false,
        gcTime: 0,
        meta: { suppressErrorToast: true },
    })

    return (
        <FilteredImportResult
            job={job}
            kind="customer"
            canReadItems={canReadItems}
            canDownload={canDownload}
            listLink={
                canList
                    ? {
                          href: "/sales/customers",
                          label: "前往客户中心",
                          onClick: () => {
                              void queryClient.invalidateQueries({
                                  queryKey: customerKeys.all,
                              })
                          },
                      }
                    : undefined
            }
            download={download}
        />
    )
}
