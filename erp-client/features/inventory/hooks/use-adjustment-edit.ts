"use client"

import { useQuery } from "@tanstack/react-query"
import { fetchEditableAdjustment } from "../api/adjustment-edit"
import { inventoryKeys } from "./queries"

export function useEditableAdjustmentQuery(id: string) {
    return useQuery({
        queryKey: [...inventoryKeys.adjustment(id), "edit"],
        queryFn: () => fetchEditableAdjustment(id),
        staleTime: 0,
        refetchOnWindowFocus: false,
    })
}
