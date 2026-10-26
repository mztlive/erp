"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    confirmCatalogIdentityReview,
    fetchCatalogIdentityReview,
} from "../api/identity-review"

export function useCatalogIdentityReviewQuery(
    connectionId: string,
    itemId: string,
) {
    return useQuery({
        queryKey: [
            "supplier-api-connections",
            "identity-review",
            connectionId,
            itemId,
        ],
        queryFn: () => fetchCatalogIdentityReview(connectionId, itemId),
        staleTime: 0,
    })
}

export function useConfirmCatalogIdentityReviewMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: confirmCatalogIdentityReview,
        onSuccess: async () => {
            await Promise.all(
                [
                    ["supplier-api-connections"],
                    ["supplier-offerings"],
                    ["master-data"],
                ].map((queryKey) =>
                    queryClient.invalidateQueries({ queryKey }),
                ),
            )
        },
    })
}
