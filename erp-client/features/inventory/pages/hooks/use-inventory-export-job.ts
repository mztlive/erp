"use client"

import * as React from "react"

import type { InventoryExportJob } from "@/features/inventory/api/inventory"
import { useStartInventoryExportMutation } from "@/features/inventory/hooks/queries"
import type { InventoryQuery } from "@/features/inventory/types"

export function useInventoryExportJob() {
    const exportMutation = useStartInventoryExportMutation()
    const [exportJob, setExportJob] = React.useState<InventoryExportJob | null>(
        null,
    )

    const startExport = React.useCallback(
        (input: {
            total: number
            filterSummary: string
            query?: InventoryQuery
        }) => {
            void exportMutation
                .mutateAsync(input)
                .then((job) => setExportJob(job))
        },
        [exportMutation],
    )

    const closeExport = React.useCallback(() => {
        setExportJob(null)
    }, [])

    return {
        exportJob,
        startExport,
        closeExport,
        isExporting: exportMutation.isPending,
    }
}
