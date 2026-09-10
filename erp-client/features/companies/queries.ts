"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { fetchCompanies, fetchCompany, saveCompany } from "./api"

export const useCompaniesQuery = (
    params: Parameters<typeof fetchCompanies>[0],
) =>
    useQuery({
        queryKey: ["companies", "list", params],
        queryFn: () => fetchCompanies(params),
    })
export const useCompanyQuery = (id?: string) =>
    useQuery({
        queryKey: ["companies", "detail", id],
        queryFn: () => fetchCompany(id!),
        enabled: Boolean(id),
    })
export const useSaveCompanyMutation = () => {
    const client = useQueryClient()
    return useMutation({
        mutationFn: saveCompany,
        retry: false,
        onSettled: async () => {
            await Promise.all([
                client.invalidateQueries({ queryKey: ["companies"] }),
                client.invalidateQueries({ queryKey: ["master-data"] }),
                client.invalidateQueries({ queryKey: ["party-selector"] }),
            ])
        },
    })
}
