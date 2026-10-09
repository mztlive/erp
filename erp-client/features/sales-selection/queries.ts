"use client"

import { useQuery } from "@tanstack/react-query"

import * as api from "./api"

export const salesSelectionKeys = {
    all: ["sales-selection"] as const,
    proposal: (id: string) =>
        [...salesSelectionKeys.all, "proposal", id] as const,
    public: (token: string, accessToken = "") =>
        [...salesSelectionKeys.all, "public", token, accessToken] as const,
}

export function useProposalQuery(id: string | undefined) {
    return useQuery({
        queryKey: salesSelectionKeys.proposal(id ?? ""),
        queryFn: () => api.fetchProposal(id!),
        enabled: Boolean(id),
    })
}

export function usePublicSelectionQuery(
    token: string | undefined,
    accessToken?: string,
    enabled = true,
) {
    return useQuery({
        queryKey: salesSelectionKeys.public(token ?? "", accessToken),
        queryFn: () => api.fetchPublicPage(token!, accessToken),
        enabled: Boolean(token) && enabled,
        retry: false,
    })
}
