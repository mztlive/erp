"use client"

import { useMutation, useQueryClient } from "@tanstack/react-query"

import { submitProductImport } from "@/features/master-data/api/product-import"
import { masterDataKeys } from "@/features/master-data/hooks/queries"

export const productImportKeys = {
    all: [...masterDataKeys.all, "product-import"] as const,
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
