"use client"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { fetchSupplierOfferingsForSkus } from "../api/offerings"
import { resolveSupplyImport, submitSupplyBatch } from "../api/batch"
import type { BatchInput, BatchMode } from "../lib/batch-supply"
import { readSupplyFile } from "../lib/batch-supply-import"
export function useSupplyBatchMutation(mode: BatchMode) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: (input: {
            rows: { row_id: string; input: BatchInput }[]
            validateOnly: boolean
            recoveryOnly?: boolean
        }) => submitSupplyBatch(mode, input.rows, input.validateOnly),
        retry: false,
        meta: { affectsDataScope: true },
        onSuccess: async (result) => {
            if (!result.rows.some((row) => row.status === "SUCCEEDED")) return
            await Promise.all([
                client.invalidateQueries({ queryKey: ["supplier-offerings"] }),
                client.invalidateQueries({ queryKey: ["master-data"] }),
            ])
        },
    })
}
export function useSupplyFileMutation() {
    return useMutation({
        mutationFn: async (file: File) =>
            resolveSupplyImport(await readSupplyFile(file)),
        retry: false,
    })
}

/** 版本冲突后重新读取当前条款，用户核对后再应用批量修改。 */
export function useReloadSupplyRowsMutation() {
    return useMutation({
        mutationFn: fetchSupplierOfferingsForSkus,
        retry: false,
    })
}
