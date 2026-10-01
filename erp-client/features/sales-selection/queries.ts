"use client"

import { useQuery } from "@tanstack/react-query"

import * as api from "./api"

export const salesSelectionKeys = {
    all: ["sales-selection"] as const,
    proposal: (id: string) =>
        [...salesSelectionKeys.all, "proposal", id] as const,
    public: (token: string) =>
        [...salesSelectionKeys.all, "public", token] as const,
}

export function useProposalQuery(id: string | undefined) {
    return useQuery({
        queryKey: salesSelectionKeys.proposal(id ?? ""),
        queryFn: () => api.fetchProposal(id!),
        enabled: Boolean(id),
    })
}

export function usePublicSelectionQuery(token: string | undefined) {
    return useQuery({
        queryKey: salesSelectionKeys.public(token ?? ""),
        queryFn: () => api.fetchPublicPage(token!),
        enabled: Boolean(token),
        retry: false,
    })
}
