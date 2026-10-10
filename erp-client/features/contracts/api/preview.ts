import { apiGetBlob } from "@/lib/api"

export function previewContractFile(contractId: string, fileId: string) {
    return apiGetBlob(
        `/admin/contracts/${encodeURIComponent(contractId)}/files/${encodeURIComponent(fileId)}/preview`,
    )
}
