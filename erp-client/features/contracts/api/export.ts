import { fetchContracts } from "./list"
import type { ContractsUrlState } from "../lib/contracts-url-state"
import { buildListCsv, collectExportPages } from "@/lib/list-export"

/** 获取完整合同结果并生成 CSV；跨页携带范围版本，下载前撤权重验。 */
export const createContractExportJob = async (input: {
    query: ContractsUrlState
    filterSnapshotLabel: string
}) => {
    let scopeVersion: string | undefined
    const rows = await collectExportPages(
        async (page, pageSize) => {
            const result = await fetchContracts({
                ...input.query,
                page,
                pageSize,
                scopeVersion,
            })
            if (result.emptyReason === "no_scope")
                throw { kind: "Auth", message: "当前无权导出合同" }
            scopeVersion = result.scopeVersion
            return { items: result.items, total: result.total }
        },
        (row) => row.contractId,
    )
    await fetchContracts({
        ...input.query,
        page: 1,
        pageSize: 1,
        scopeVersion,
    })
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
