"use client"

import { useQuery } from "@tanstack/react-query"
import {
    matchImportIdentities,
    fetchImportCustomer,
} from "../api/import-identities"
import type { ContractImportTask } from "../api/upload"

export function useImportIdentityMatches(
    task: ContractImportTask,
    expectedCustomerId?: string,
) {
    const fields = task.draft?.fields ?? {}
    const input = {
        customer: {
            name: fields.customer_name ?? "",
            code: fields.customer_credit_code ?? "",
        },
        company: {
            name: fields.company_name ?? "",
            code: fields.company_credit_code ?? "",
        },
        expectedCustomerId,
    }
    return useQuery({
        queryKey: ["contract-import-identities", task.id, input],
        queryFn: () => matchImportIdentities(input),
        retry: false,
    })
}

export function useImportCustomer(id: string) {
    return useQuery({
        queryKey: ["contract-import-identities", "customer", id],
        queryFn: () => fetchImportCustomer(id),
        enabled: Boolean(id),
    })
}
