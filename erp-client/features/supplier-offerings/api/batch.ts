import { transferJson } from "@/lib/data-transfer"
import { apiPost } from "@/lib/api"
import type { BatchInput, BatchMode, BatchResult } from "../lib/batch-supply"

export function submitSupplyBatch(
    mode: BatchMode,
    rows: { row_id: string; input: BatchInput }[],
    validateOnly: boolean,
) {
    if (!validateOnly)
        return transferJson<BatchResult>("supply_batch", {
            mode,
            rows,
            validate_only: false,
        })
    return apiPost<BatchResult>(`/admin/supplier-offerings/batch/${mode}`, {
        rows,
        validate_only: validateOnly,
    })
}
