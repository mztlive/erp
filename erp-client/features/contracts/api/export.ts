import { contractExportQuery } from "./list"
import type { ContractsUrlState } from "../lib/contracts-url-state"
import { transferCsv } from "@/lib/data-transfer"
export const createContractExportJob = async (input: {
    query: ContractsUrlState
    filterSnapshotLabel: string
}) => {
    const result = await transferCsv(
        "contracts",
        contractExportQuery(input.query),
    )
    return {
        ...result,
        status: "succeeded" as const,
        filterSnapshotLabel: input.filterSnapshotLabel,
        createdAt: new Date().toISOString(),
        downloadLabel: result.fileName,
    }
}
