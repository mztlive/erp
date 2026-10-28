"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    createAdoptionBatch,
    fetchAdoptionBatch,
    fetchAdoptionBatches,
    previewAdoptionBatch,
    queryAdoptionBatchSubmission,
    reconcileAdoptionBatch,
    saveAdoptionBatch,
    submitAdoptionBatch,
    type BatchIdentity,
    type BatchView,
} from "../api/batch"

export const adoptionBatchKeys = {
    all: ["supplier-api-connections", "adoption-batches"] as const,
    detail: (identity: BatchIdentity) =>
        [
            ...adoptionBatchKeys.all,
            identity.connectionId,
            identity.batchId,
        ] as const,
}
export function useAdoptionBatchesQuery(connectionId: string) {
    return useQuery({
        queryKey: [...adoptionBatchKeys.all, connectionId, "list"],
        queryFn: () => fetchAdoptionBatches(connectionId),
        enabled: Boolean(connectionId),
    })
}
export function useAdoptionBatchQuery(identity: BatchIdentity) {
    return useQuery({
        queryKey: adoptionBatchKeys.detail(identity),
        queryFn: () => fetchAdoptionBatch(identity),
        enabled: Boolean(identity.connectionId && identity.batchId),
        refetchInterval: (query) =>
            query.state.data?.batch.draft_operation ||
            query.state.data?.batch.execution_pending ||
            query.state.data?.rows.some(
                (row) =>
                    row.status === "PROCESSING" ||
                    (row.status === "READY" && Boolean(row.operation_id)),
            )
                ? 2000
                : false,
    })
}
function useBatchResult() {
    const queryClient = useQueryClient()
    return (view: BatchView) => {
        queryClient.setQueryData(
            adoptionBatchKeys.detail({
                connectionId: view.batch.connection_id,
                batchId: view.batch.id,
            }),
            view,
        )
        void queryClient.invalidateQueries({
            queryKey: [
                ...adoptionBatchKeys.all,
                view.batch.connection_id,
                "list",
            ],
        })
        void queryClient.invalidateQueries({ queryKey: ["master-data"] })
        void queryClient.invalidateQueries({ queryKey: ["supplier-offerings"] })
    }
}
export function useCreateAdoptionBatchMutation() {
    const onSuccess = useBatchResult()
    return useMutation({ mutationFn: createAdoptionBatch, onSuccess })
}
export function useSaveAdoptionBatchMutation() {
    const onSuccess = useBatchResult()
    return useMutation({ mutationFn: saveAdoptionBatch, onSuccess })
}
export function usePreviewAdoptionBatchMutation() {
    const onSuccess = useBatchResult()
    return useMutation({ mutationFn: previewAdoptionBatch, onSuccess })
}
export function useSubmitAdoptionBatchMutation() {
    const onSuccess = useBatchResult()
    return useMutation({
        mutationFn: submitAdoptionBatch,
        onSuccess,
        retry: false,
    })
}
export function useReconcileAdoptionBatchMutation() {
    const onSuccess = useBatchResult()
    return useMutation({
        mutationFn: reconcileAdoptionBatch,
        onSuccess,
        retry: false,
    })
}

export function useQueryAdoptionBatchSubmissionMutation() {
    const onSuccess = useBatchResult()
    return useMutation({
        mutationFn: queryAdoptionBatchSubmission,
        onSuccess: (result) => onSuccess(result.view),
        retry: false,
    })
}
