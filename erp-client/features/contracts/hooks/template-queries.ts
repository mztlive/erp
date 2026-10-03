"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    applyContractTemplate,
    configureContractCounter,
    downloadContractWord,
    fetchContractApplications,
    fetchContractCounters,
    fetchContractTemplates,
    setContractTemplateStatus,
    uploadContractTemplate,
    type TemplatePageParams,
} from "../api/templates"

export const templateKeys = { all: ["contract-templates"] as const }
export const useContractTemplatesQuery = (
    params: TemplatePageParams,
    enabled: boolean,
) =>
    useQuery({
        queryKey: [...templateKeys.all, "list", params],
        queryFn: () => fetchContractTemplates(params),
        enabled,
    })
export const useContractApplicationsQuery = (
    params: TemplatePageParams,
    accountId: string,
    enabled: boolean,
) =>
    useQuery({
        queryKey: [...templateKeys.all, "applications", accountId, params],
        queryFn: () => fetchContractApplications(params),
        enabled: enabled && Boolean(accountId),
    })
export const useContractCountersQuery = (enabled: boolean) =>
    useQuery({
        queryKey: [...templateKeys.all, "counters"],
        queryFn: fetchContractCounters,
        enabled,
    })

export function useTemplateMutations() {
    const client = useQueryClient()
    const refresh = () =>
        client.invalidateQueries({ queryKey: templateKeys.all })
    return {
        upload: useMutation({
            mutationFn: uploadContractTemplate,
            retry: false,
            onSettled: refresh,
        }),
        apply: useMutation({
            mutationFn: applyContractTemplate,
            retry: false,
            onSettled: refresh,
        }),
        status: useMutation({
            mutationFn: setContractTemplateStatus,
            retry: false,
            onSettled: refresh,
        }),
        counter: useMutation({
            mutationFn: configureContractCounter,
            retry: false,
            onSettled: refresh,
        }),
        download: useMutation({
            mutationFn: downloadContractWord,
            retry: false,
        }),
    }
}
