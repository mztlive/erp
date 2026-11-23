"use client"

import { useMutation } from "@tanstack/react-query"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { FileUpload } from "@/components/ui/file-upload"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useCustomerImportMutation } from "@/features/customers/hooks/queries"
import {
    CUSTOMER_IMPORT_MAX_BYTES,
    downloadCustomerImportTemplate,
    readCustomerImportWorkbook,
} from "@/features/customers/lib/customer-import-workbook"
import { getErrorMessage, isApiError } from "@/lib/api/errors"
import type { BackgroundJobView } from "@/lib/background-jobs"

export function CustomerImportDialog({
    onClose,
    onSubmitted,
}: {
    onClose: () => void
    onSubmitted: (job: BackgroundJobView, fileName: string) => void
}) {
    const submit = useCustomerImportMutation()
    const read = useMutation({
        mutationFn: async (file: File) => ({
            file,
            request_id: crypto.randomUUID(),
            rows: await readCustomerImportWorkbook(file),
        }),
        onMutate: () => submit.reset(),
        meta: { suppressErrorToast: true },
    })
    const template = useMutation({
        mutationFn: downloadCustomerImportTemplate,
        meta: { suppressErrorToast: true },
    })
    const source = read.data
    const rows = source?.rows ?? []
    const invalidCount = rows.filter((row) => row.error).length
    const busy = read.isPending || submit.isPending
    const submitError = submit.error
    const uncertain =
        isApiError(submitError) &&
        (submitError.kind === "Network" ||
            submitError.kind === "Parse" ||
            submitError.status === 408 ||
            (submitError.status ?? 0) >= 500)

    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !busy) onClose()
            }}
        >
            <DialogContent
                id="customers-import-dialog"
                size="xl"
                closeButtonId="customers-import-dismiss"
                showCloseButton={!busy}
            >
                <DialogHeader>
                    <DialogTitle>批量导入客户</DialogTitle>
                    <DialogDescription>
                        每行导入一个客户的名称、简称和统一社会信用代码。新客户默认启用，负责销售为提交人。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-4">
                    <LoadingButton
                        id="customers-import-template"
                        type="button"
                        variant="outline"
                        loading={template.isPending}
                        onClick={() => template.mutate()}
                    >
                        下载导入模板
                    </LoadingButton>
                    <FileUpload
                        idPrefix="customers-import-file"
                        inputId="customers-import-file"
                        label="选择客户 Excel"
                        description="支持 XLSX，最多 1000 行、5 MB。"
                        density="compact"
                        multiple={false}
                        accept=".xlsx"
                        maxFileSize={CUSTOMER_IMPORT_MAX_BYTES}
                        disabled={busy || uncertain}
                        selectedFiles={source ? [source.file] : []}
                        onFilesSelected={(files) => {
                            if (files[0]) read.mutate(files[0])
                        }}
                        onFileRemove={() => {
                            read.reset()
                            submit.reset()
                        }}
                    />
                    {read.isPending && (
                        <p role="status" className="text-body-compact">
                            正在读取文件…
                        </p>
                    )}
                    {(read.error || template.error || submitError) && (
                        <p
                            role="alert"
                            className="text-body-compact text-destructive"
                        >
                            {uncertain
                                ? "暂未确认任务是否提交成功，请点击「重新确认」。"
                                : getErrorMessage(
                                      read.error ??
                                          submitError ??
                                          template.error,
                                      "操作失败，请重试",
                                  )}
                        </p>
                    )}
                    {source && (
                        <>
                            <p role="status" className="text-body-compact">
                                共 {rows.length} 行
                                {invalidCount > 0 &&
                                    `，${invalidCount} 行需修正`}
                            </p>
                            <div className="overflow-hidden rounded-lg border [&>[data-slot=table-container]]:max-h-72">
                                <Table data-density="compact">
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>Excel 行号</TableHead>
                                            <TableHead>客户名称</TableHead>
                                            <TableHead>客户简称</TableHead>
                                            <TableHead>
                                                统一社会信用代码
                                            </TableHead>
                                            <TableHead>读取结果</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {rows.map((row) => (
                                            <TableRow key={row.row_number}>
                                                <TableCell className="num">
                                                    {row.row_number}
                                                </TableCell>
                                                <TableCell className="max-w-48 whitespace-normal break-all">
                                                    {row.legal_name || "—"}
                                                </TableCell>
                                                <TableCell className="max-w-32 whitespace-normal break-all">
                                                    {row.short_name || "—"}
                                                </TableCell>
                                                <TableCell className="max-w-44 whitespace-normal break-all">
                                                    {row.unified_credit_code ||
                                                        "—"}
                                                </TableCell>
                                                <TableCell className="max-w-52 whitespace-normal">
                                                    {row.error ?? "待核对"}
                                                </TableCell>
                                            </TableRow>
                                        ))}
                                    </TableBody>
                                </Table>
                            </div>
                        </>
                    )}
                    <div className="flex flex-wrap justify-end gap-2">
                        <Button
                            id="customers-import-close"
                            type="button"
                            variant="outline"
                            disabled={busy}
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <LoadingButton
                            id="customers-import-submit"
                            type="button"
                            loading={submit.isPending}
                            disabled={busy || !source}
                            onClick={() => {
                                if (!source) return
                                submit.mutate(
                                    {
                                        request_id: source.request_id,
                                        file_name: source.file.name,
                                        rows: source.rows,
                                    },
                                    {
                                        onSuccess: (job) => {
                                            onSubmitted(job, source.file.name)
                                            onClose()
                                        },
                                    },
                                )
                            }}
                        >
                            {uncertain ? "重新确认" : "提交后台导入"}
                        </LoadingButton>
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}
