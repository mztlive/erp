"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchProcurementResponsibilityRules,
    saveProcurementResponsibilityRule,
} from "@/features/procurement-responsibilities/api/rules"

export const procurementResponsibilityKeys = {
    all: ["procurement-responsibility-rules"] as const,
    list: () => [...procurementResponsibilityKeys.all, "list"] as const,
}

export function useProcurementResponsibilityRulesQuery() {
    return useQuery({
        queryKey: procurementResponsibilityKeys.list(),
        queryFn: fetchProcurementResponsibilityRules,
    })
}

export function useSaveProcurementResponsibilityRuleMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: saveProcurementResponsibilityRule,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: procurementResponsibilityKeys.all,
            })
        },
    })
}
