"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { approvalKeys } from "@/features/approval-workflow/queries"
import { workItemKeys } from "@/features/work-items/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    fetchFinancialDraft,
    saveFinancialDraft,
    submitFinancialDraft,
} from "../api"
import type { FinancialDraft, FinancialDraftKind } from "../types"

const financialDraftKeys = {
    all: ["financial-draft-edit"] as const,
    detail: (kind: FinancialDraftKind, id: string) =>
        [...financialDraftKeys.all, kind, id] as const,
}

/** 编辑窗口只读取当前原单，不加载新的来源或新建草稿。 */
export function useFinancialDraftQuery(
    kind: FinancialDraftKind,
    id: string,
    enabled = true,
) {
    return useQuery({
        queryKey: financialDraftKeys.detail(kind, id),
        queryFn: () => fetchFinancialDraft(kind, id),
        enabled: enabled && Boolean(id),
        staleTime: 0,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
    })
}

/** 保存与重提分别更新当前版本，并刷新财务列表、审批及工作台。 */
export function useFinancialDraftMutations(
    kind: FinancialDraftKind,
    id: string,
) {
    const queryClient = useQueryClient()
    const onSuccess = async (draft: FinancialDraft) => {
        queryClient.setQueryData(financialDraftKeys.detail(kind, id), draft)
        await Promise.all([
            queryClient.invalidateQueries({
                queryKey: queryKeyRoots.customerReceivables,
            }),
            queryClient.invalidateQueries({
                queryKey: queryKeyRoots.supplierPayables,
            }),
            queryClient.invalidateQueries({ queryKey: approvalKeys.all }),
            queryClient.invalidateQueries({ queryKey: workItemKeys.all }),
            queryClient.invalidateQueries({
                queryKey: queryKeyRoots.workspaceHome,
            }),
        ])
    }
    const save = useMutation({ mutationFn: saveFinancialDraft, onSuccess })
    const submit = useMutation({ mutationFn: submitFinancialDraft, onSuccess })
    return { save, submit, confirm: onSuccess }
}
