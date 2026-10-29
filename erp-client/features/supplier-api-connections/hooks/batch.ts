"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    createAdoptionBatch,
    fetchBatchPreparation,
    fetchBatchSelection,
    controlBatchPreparation,
    controlBatchWork,
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
export function useAdoptionBatchesQuery(connectionId: string, refresh = false) {
    return useQuery({
        queryKey: [...adoptionBatchKeys.all, connectionId, "list"],
        queryFn: () => fetchAdoptionBatches(connectionId),
        enabled: Boolean(connectionId),
        refetchInterval: refresh ? 5000 : false,
    })
}
export function useAdoptionBatchQuery(identity: BatchIdentity, enabled = true) {
    return useQuery({
        queryKey: adoptionBatchKeys.detail(identity),
        queryFn: () => fetchAdoptionBatch(identity),
        staleTime: 0,
        enabled: enabled && Boolean(identity.connectionId && identity.batchId),
        refetchInterval: (query) =>
            query.state.data?.batch.work &&
            query.state.data.batch.work.status !== "COMPLETED"
                ? false
                : query.state.data?.batch.draft_operation ||
                    query.state.data?.batch.execution_pending ||
                    query.state.data?.rows.some(
                        (row) =>
                            row.status === "PROCESSING" ||
                            (row.status === "READY" &&
                                Boolean(row.operation_id)),
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
            (previous: BatchView | undefined) =>
                view.rows_complete === false && previous
                    ? {
                          ...previous,
                          batch: view.batch,
                          rows_complete:
                              view.batch.work &&
                              (previous.batch.work?.status !== "COMPLETED" ||
                                  previous.batch.work?.id !==
                                      view.batch.work.id)
                                  ? false
                                  : previous.rows_complete,
                      }
                    : view,
        )
        queryClient.setQueryData(
            [
                ...adoptionBatchKeys.detail({
                    connectionId: view.batch.connection_id,
                    batchId: view.batch.id,
                }),
                "preparation",
            ],
            view.batch,
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
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: createAdoptionBatch,
        retry: false,
        onSuccess: (view) => {
            queryClient.setQueryData(
                [
                    ...adoptionBatchKeys.detail({
                        connectionId: view.batch.connection_id,
                        batchId: view.batch.id,
                    }),
                    "preparation",
                ],
                view.batch,
            )
            void queryClient.invalidateQueries({
                queryKey: [
                    ...adoptionBatchKeys.all,
                    view.batch.connection_id,
                    "list",
                ],
            })
        },
    })
}
export function useBatchPreparationQuery(identity: BatchIdentity) {
    return useQuery({
        queryKey: [...adoptionBatchKeys.detail(identity), "preparation"],
        queryFn: () => fetchBatchPreparation(identity),
        enabled: Boolean(identity.connectionId && identity.batchId),
        refetchInterval: (query) =>
            query.state.data?.work?.status === "RUNNING" ||
            ["QUEUED", "PREPARING"].includes(
                query.state.data?.preparation?.status ?? "",
            )
                ? 2000
                : false,
    })
}
export function useControlBatchPreparationMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: controlBatchPreparation,
        retry: false,
        onSuccess: (batch) => {
            queryClient.setQueryData(
                [
                    ...adoptionBatchKeys.detail({
                        connectionId: batch.connection_id,
                        batchId: batch.id,
                    }),
                    "preparation",
                ],
                batch,
            )
            void queryClient.invalidateQueries({
                queryKey: [
                    ...adoptionBatchKeys.all,
                    batch.connection_id,
                    "list",
                ],
            })
        },
    })
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

export function useBatchSelectionQuery(
    identity: BatchIdentity,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [...adoptionBatchKeys.detail(identity), "selection"],
        queryFn: () => fetchBatchSelection(identity),
        staleTime: 0,
        enabled: enabled && Boolean(identity.batchId && identity.connectionId),
    })
}

export function useControlBatchWorkMutation() {
    const onSuccess = useBatchResult()
    return useMutation({
        mutationFn: controlBatchWork,
        retry: false,
        onSuccess,
    })
}
