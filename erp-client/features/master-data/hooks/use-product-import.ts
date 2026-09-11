"use client"

import { useEffect, useRef } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchProductImportItems,
    fetchProductImportJobs,
    submitProductImport,
} from "@/features/master-data/api/product-import"
import { isProductImportActive } from "@/features/master-data/lib/product-import"
import { masterDataKeys } from "@/features/master-data/hooks/queries"

export const productImportKeys = {
    all: [...masterDataKeys.all, "product-import"] as const,
    jobs: () => [...productImportKeys.all, "jobs"] as const,
    items: (id: string) => [...productImportKeys.all, "items", id] as const,
}

export function useProductImportJobsQuery(enabled: boolean) {
    const client = useQueryClient()
    const wasActive = useRef(false)
    const query = useQuery({
        queryKey: productImportKeys.jobs(),
        queryFn: () => fetchProductImportJobs(1),
        enabled,
        refetchInterval: (current) => {
            const active = current.state.data?.items.some((job) =>
                isProductImportActive(job.status),
            )
            return active ? 2000 : false
        },
    })
    const active = query.data?.items.some((job) =>
        isProductImportActive(job.status),
    )
    useEffect(() => {
        if (wasActive.current && !active) {
            void client.invalidateQueries({ queryKey: masterDataKeys.all })
        }
        wasActive.current = Boolean(active)
    }, [active, client])
    return query
}

export function useProductImportItemsQuery(jobId: string | null) {
    return useQuery({
        queryKey: productImportKeys.items(jobId ?? ""),
        queryFn: () => fetchProductImportItems(jobId ?? ""),
        enabled: Boolean(jobId),
        refetchInterval: 2000,
    })
}

export function useSubmitProductImportMutation() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: submitProductImport,
        retry: false,
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: productImportKeys.all })
            await client.invalidateQueries({ queryKey: masterDataKeys.all })
        },
    })
}
