"use client"
import { useQuery } from "@tanstack/react-query"
import type { ContractImportTask } from "@/features/contracts/api/upload"
import { matchSalesContract } from "../api/contract-prefill"

export function useSalesContractMatches(task: ContractImportTask | undefined) {
    return useQuery({
        queryKey: ["sales-orders", "contract-prefill", task?.id, task?.version],
        queryFn: () => matchSalesContract(task!),
        enabled: Boolean(task),
        staleTime: 0,
        refetchOnWindowFocus: false,
    })
}
