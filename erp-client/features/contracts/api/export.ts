import { fetchContracts } from "./list"
import type { ContractsUrlState } from "../lib/contracts-url-state"
import { buildListCsv, collectExportPages } from "@/lib/list-export"

/** 获取完整合同结果并生成 CSV；每页均通过原列表权限校验。 */
export const createContractExportJob = async (input: {
    query: ContractsUrlState
    filterSnapshotLabel: string
}) => {
    const rows = await collectExportPages(
        (page, pageSize) => fetchContracts({ ...input.query, page, pageSize }),
        (row) => row.contractId,
    )
    const content = buildListCsv([
        [
            "合同编号",
            "客户",
            "结算主体",
            "状态",
            "当前跟进负责人",
            "有效期起",
            "有效期止",
        ],
        ...rows.map((row) => [
            row.contractNo,
            row.customer.displayName,
            row.settlementParty.displayName,
            row.statusLabel,
            row.ownerLabel,
            row.validFrom,
            row.validTo,
        ]),
    ])
    return {
        jobId: "",
        status: "succeeded" as const,
        rowCount: rows.length,
        filterSnapshotLabel: input.filterSnapshotLabel,
        createdAt: new Date().toISOString(),
        downloadLabel: "合同列表.csv",
        content,
    }
}
