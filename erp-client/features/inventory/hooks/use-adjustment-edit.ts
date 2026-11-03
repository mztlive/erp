"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { fetchEditableAdjustment, saveAdjustment } from "../api/adjustment-edit"
import { inventoryKeys } from "./queries"

export function useEditableAdjustmentQuery(id: string) {
    return useQuery({
        queryKey: [...inventoryKeys.adjustment(id), "edit"],
        queryFn: () => fetchEditableAdjustment(id),
        staleTime: 0,
        refetchOnWindowFocus: false,
    })
}

/** 保存后刷新单据与列表，保留已驳回状态。 */
export function useSaveAdjustmentMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: saveAdjustment,
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: inventoryKeys.all })
        },
    })
}
