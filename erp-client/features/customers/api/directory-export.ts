import { fetchCustomerDirectory } from "./directory"
import type { CustomerDirectoryQuery } from "../types"
import { buildListCsv, collectExportPages } from "@/lib/list-export"

/** 导出目录可见字段；当前主责、状态、组织与人员条件贯穿所有页。 */
export const exportCustomerDirectory = async (
    query: CustomerDirectoryQuery,
) => {
    let scopeVersion: string | undefined
    const rows = await collectExportPages(
        async (page, pageSize) => {
            const result = await fetchCustomerDirectory({
                ...query,
                page,
                pageSize,
                scopeVersion,
            })
            if (result.emptyReason === "no_scope")
                throw { kind: "Auth", message: "当前无权导出客户" }
            scopeVersion = result.scopeVersion
            return { items: result.items, total: result.totalInScope }
        },
        (row) => row.id,
    )
    await fetchCustomerDirectory({
        ...query,
        page: 1,
        pageSize: 1,
        scopeVersion,
    })
    return buildListCsv([
        ["客户编号", "客户名称", "状态", "负责销售", "协作人数"],
        ...rows.map((row) => [
            row.customerNo,
            row.legalName,
            row.statusLabel.label,
            row.ownerName,
            String(row.collaboratorCount),
        ]),
    ])
}
