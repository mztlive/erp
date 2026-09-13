import { fetchCustomerDirectory } from "./directory"
import type { CustomerDirectoryQuery } from "../types"
import { buildListCsv, collectExportPages } from "@/lib/list-export"

/** 导出目录可见字段；当前主责、状态与人员条件贯穿所有页。 */
export const exportCustomerDirectory = async (
    query: CustomerDirectoryQuery,
) => {
    const rows = await collectExportPages(
        async (page, pageSize) => {
            const result = await fetchCustomerDirectory({
                ...query,
                page,
                pageSize,
            })
            if (!result.hasCustomerScope)
                throw { kind: "Auth", message: "当前无权导出客户" }
            return { items: result.items, total: result.totalInScope }
        },
        (row) => row.id,
    )
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
