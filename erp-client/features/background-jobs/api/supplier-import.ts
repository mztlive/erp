import { apiGet } from "@/lib/api"
import type {
    SupplierImportResult,
    SupplierImportRow,
} from "@/lib/supplier-import"

/** 由服务端重验原提交人、创建权限与任务终态后返回待处理源行。 */
export function fetchSupplierImportFailures(jobId: string) {
    return apiGet<{
        rows: SupplierImportRow[]
        results: SupplierImportResult[]
    }>(
        `/admin/supplier-profiles/import/jobs/${encodeURIComponent(jobId)}/failures`,
    )
}
