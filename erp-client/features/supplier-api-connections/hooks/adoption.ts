"use client"

import { useMutation, useQueryClient } from "@tanstack/react-query"
import { confirmAdoption, previewAdoption } from "../api/adoption"

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
