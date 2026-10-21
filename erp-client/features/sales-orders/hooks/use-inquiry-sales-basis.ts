"use client"
import { useQuery } from "@tanstack/react-query"
import { fetchInquirySalesBasis } from "../api/inquiry-basis"

export function useInquirySalesBasis(id: string, round: number) {
    return useQuery({
        queryKey: ["sales-orders", "inquiry-basis", id, round],
        queryFn: () => fetchInquirySalesBasis(id, round),
        enabled: Boolean(id) && round > 0,
        staleTime: 0,
        retry: false,
    })
}
