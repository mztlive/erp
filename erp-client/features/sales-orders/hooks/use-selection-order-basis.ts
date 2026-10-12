"use client"
import { useQuery } from "@tanstack/react-query"
import { fetchSelectionOrderBasis } from "@/features/sales-orders/api/selection-basis"

export function useSelectionOrderBasis(id: string) {
    return useQuery({
        queryKey: ["sales-orders", "selection-basis", id],
        queryFn: () => fetchSelectionOrderBasis(id),
        enabled: Boolean(id),
        retry: false,
    })
}
