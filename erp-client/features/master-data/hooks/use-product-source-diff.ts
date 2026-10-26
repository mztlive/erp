"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    fetchProductSourceDiff,
    resolveProductSourceDiff,
} from "../api/product-source"
import type { ResolveProductSourceDiff } from "../api/types-product-source"
import { masterDataKeys } from "./queries"

export const productSourceKeys = {
    all: ["product-source-diff"] as const,
    detail: (productId: string) => ["product-source-diff", productId] as const,
}

export function useProductSourceDiffQuery(productId: string, enabled: boolean) {
    return useQuery({
        queryKey: productSourceKeys.detail(productId),
        queryFn: () => fetchProductSourceDiff(productId),
        enabled: enabled && Boolean(productId),
    })
}

export function useResolveProductSourceDiffMutation(productId: string) {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (request: ResolveProductSourceDiff) =>
            resolveProductSourceDiff(productId, request),
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({ queryKey: masterDataKeys.all }),
                queryClient.invalidateQueries({
                    queryKey: productSourceKeys.detail(productId),
                }),
            ])
        },
    })
}
