"use client"

import { useMutation } from "@tanstack/react-query"

import { toast } from "@/components/ui/toast"
import { uploadEvidenceFileAsset } from "@/features/file-assets/api"
import { downloadSalesOrderEvidence } from "@/features/sales-orders/api/sales-order-evidence"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { getErrorMessage } from "@/lib/api/errors"

/** 上传成功身份保留在建单表单中，后续保存与结果未知重试复用该身份。 */
export function useSalesOrderEvidenceUpload(form: SalesOrderCreateFormApi) {
    return useMutation({
        mutationFn: uploadEvidenceFileAsset,
        onMutate: () => form.setFieldValue("evidenceUploadPending", true),
        onSettled: () => form.setFieldValue("evidenceUploadPending", false),
        onSuccess: (asset) => {
            const current = form.state.values.evidenceAttachments ?? []
            if (current.some((item) => item.id === asset.id)) return
            form.setFieldValue("evidenceAttachments", [
                ...current,
                { id: asset.id, fileName: asset.file_name },
            ])
        },
        onError: (error) =>
            toast.add({
                title: "开单凭证上传失败",
                description: getErrorMessage(error, "请重新上传 PDF 或图片"),
                type: "error",
            }),
    })
}

/** 每次下载按销售单来源重新核对当前凭证关系。 */
export function useSalesOrderEvidenceDownload(salesOrderId: string) {
    return useMutation({
        mutationFn: (input: { id: string; fileName: string }) =>
            downloadSalesOrderEvidence(salesOrderId, input.id, input.fileName),
        onError: (error) =>
            toast.add({
                title: "开单凭证下载失败",
                description: getErrorMessage(error, "请重新打开销售单后重试"),
                type: "error",
            }),
    })
}
