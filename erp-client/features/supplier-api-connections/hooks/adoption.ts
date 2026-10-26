"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    confirmAdoption,
    fetchAdoptionTarget,
    previewAdoption,
} from "../api/adoption"

export function usePreviewAdoptionMutation() {
    return useMutation({ mutationFn: previewAdoption })
}

export function useConfirmAdoptionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: confirmAdoption,
        onSuccess: async () => {
            await Promise.all(
                [
                    ["supplier-api-connections"],
                    ["master-data"],
                    ["supplier-offerings"],
                    ["entity-selectors"],
                ].map((queryKey) =>
                    queryClient.invalidateQueries({ queryKey }),
                ),
            )
        },
    })
}

export function useAdoptionTargetQuery(connectionId: string, skuId: string) {
    return useQuery({
        queryKey: [
            "supplier-api-connections",
            "adoption-target",
            connectionId,
            skuId,
        ],
        queryFn: () => fetchAdoptionTarget(connectionId, skuId),
        enabled: Boolean(connectionId && skuId),
        staleTime: 0,
    })
}
