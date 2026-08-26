"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchFinanceResponsibilityOwnerOptions,
    fetchFinanceResponsibilityRules,
    saveFinanceResponsibilityRule,
} from "@/features/finance-responsibilities/api/rules"

const financeResponsibilityKeys = {
    all: ["finance-responsibilities"] as const,
    rules: () => [...financeResponsibilityKeys.all, "rules"] as const,
    owners: () => [...financeResponsibilityKeys.all, "owners"] as const,
}

export function useFinanceResponsibilityRulesQuery(enabled = true) {
    return useQuery({
        queryKey: financeResponsibilityKeys.rules(),
        queryFn: fetchFinanceResponsibilityRules,
        enabled,
    })
}

export function useFinanceResponsibilityOwnerOptionsQuery(enabled = true) {
    return useQuery({
        queryKey: financeResponsibilityKeys.owners(),
        queryFn: fetchFinanceResponsibilityOwnerOptions,
        enabled,
    })
}

export function useSaveFinanceResponsibilityRuleMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: saveFinanceResponsibilityRule,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: financeResponsibilityKeys.rules(),
            })
        },
    })
}
