"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    getArrangement,
    saveArrangement,
    type ArrangementScope,
    type SaveArrangement,
} from "../api"
const root = ["delivery-arrangements"] as const
export function useArrangement(
    scope: ArrangementScope,
    id: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [...root, scope, id],
        queryFn: () => getArrangement(scope, id),
        enabled,
        staleTime: 0,
        refetchOnWindowFocus: false,
    })
}
export function useSaveArrangement(scope: ArrangementScope, id: string) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: (request: SaveArrangement) =>
            saveArrangement(scope, id, request),
        onSuccess: async () => {
            await Promise.all(
                [
                    root,
                    queryKeyRoots.salesOrders,
                    queryKeyRoots.fulfillmentOperations,
                    queryKeyRoots.purchaseOrders,
                    queryKeyRoots.workItems,
                    queryKeyRoots.workspaceHome,
                ].map((queryKey) => client.invalidateQueries({ queryKey })),
            )
        },
    })
}
