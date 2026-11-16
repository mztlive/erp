"use client"

import { useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { FileUpload } from "@/components/ui/file-upload"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { BrandImportRow } from "@/features/master-data/api/brand-import"
import { useBrandImportMutation } from "@/features/master-data/hooks/queries"
import {
    readBrandFile,
    downloadBrandTemplate,
    downloadBrandFailures,
} from "@/features/master-data/lib/brand-import"
import { getErrorMessage } from "@/lib/api/errors"

import type { BackgroundJobView } from "@/lib/background-jobs"

export function BrandImportDialog({
    onClose,
    onSubmitted,
}: {
    onClose: () => void
    onSubmitted: (job: BackgroundJobView, fileName: string) => void
}) {
    const [source, setSource] = useState<{
        file: File
        requestId: string
        rows: BrandImportRow[]
    } | null>(null)
    const mutation = useBrandImportMutation()
    const read = useMutation({
        mutationFn: async (file: File) => ({
            file,
            requestId: crypto.randomUUID(),
            rows: await readBrandFile(file),
        }),
        onMutate: () => {
            setSource(null)
            mutation.reset()
        },
        onSuccess: setSource,
    })
    const download = useMutation({
        mutationFn: (kind: "template" | "failures") =>
            kind === "template"
                ? downloadBrandTemplate()
                : downloadBrandFailures(source?.rows ?? [], []),
    })
    const busy = read.isPending || mutation.isPending
    const uncertain = mutation.isError
    const locked = busy || uncertain
    const rows = source?.rows ?? []
    const failed = rows.filter((row) => row.error).length
    const error = read.error ?? download.error
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !busy) onClose()
            }}
        >
            <DialogContent
                id="brand-import-dialog"
                closeButtonId="brand-import-dismiss"
                className="max-h-[90dvh] overflow-y-auto sm:max-w-4xl"
            >
                <DialogHeader>
                    <DialogTitle>批量导入品牌</DialogTitle>
                    <DialogDescription>
                        每行一个品牌，名称必填，代码留空自动生成。新品牌默认启用，Logo
                        可稍后补充。已有品牌核对后跳过，不修改原资料。提交后可关闭页面，进度、逐行结果及待处理行下载在「后台任务」查看。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-4">
                    <Button
                        id="brand-import-template"
                        variant="outline"
                        disabled={download.isPending}
                        onClick={() => download.mutate("template")}
                    >
                        下载导入模板
                    </Button>
                    <FileUpload
                        idPrefix="brand-import-file"
                        inputId="brand-import-file"
                        label="选择品牌 Excel"
                        description="支持 XLSX，最多 1000 行、5 MB；也可使用只有「品牌名称」一列的文件。"
                        density="compact"
                        multiple={false}
                        accept=".xlsx"
                        disabled={locked}
                        selectedFiles={source ? [source.file] : []}
                        onFilesSelected={(files) => {
                            if (files[0]) read.mutate(files[0])
                        }}
                        onFileRemove={() => {
                            setSource(null)
                            read.reset()
                            mutation.reset()
                        }}
                    />
                    {read.isPending && (
                        <p role="status" className="text-sm">
                            正在读取文件…
                        </p>
                    )}
                    {error && (
                        <p role="alert" className="text-sm text-destructive">
                            {getErrorMessage(error, "操作失败，请重试")}
                        </p>
                    )}
                    {uncertain && (
                        <p role="alert" className="text-sm text-destructive">
                            {getErrorMessage(mutation.error, "提交响应未收到")}
                            。网络响应超时，暂未确认是否提交成功，请点击「重新确认」（不会重复创建记录）。
                        </p>
                    )}
                    {source && (
                        <p role="status" aria-live="polite" className="text-sm">
                            共 {rows.length} 行 ，{failed}{" "}
                            行读取异常。提交后由后台逐行核对并保存结果。
                        </p>
                    )}
                    {rows.length > 0 && (
                        <div className="overflow-hidden rounded-lg border [&>[data-slot=table-container]]:max-h-80 [&>[data-slot=table-container]]:overflow-auto">
                            <Table data-density="compact">
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>Excel 行</TableHead>
                                        <TableHead>品牌名称</TableHead>
                                        <TableHead>品牌代码</TableHead>
                                        <TableHead>结果</TableHead>
                                        <TableHead>说明</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {rows.map((row) => {
                                        return (
                                            <TableRow key={row.row_number}>
                                                <TableCell className="num">
                                                    {row.row_number}
                                                </TableCell>
                                                <TableCell className="whitespace-normal">
                                                    {row.name || "未填写"}
                                                </TableCell>
                                                <TableCell>
                                                    {row.brand_code ||
                                                        "自动生成"}
                                                </TableCell>
                                                <TableCell>
                                                    {row.error
                                                        ? "读取失败"
                                                        : "待提交"}
                                                </TableCell>
                                                <TableCell className="max-w-80 whitespace-normal">
                                                    {row.error ||
                                                        "提交时核对已有品牌"}
                                                </TableCell>
                                            </TableRow>
                                        )
                                    })}
                                </TableBody>
                            </Table>
                        </div>
                    )}
                    <div className="flex flex-wrap justify-end gap-2">
                        {failed > 0 && (
                            <Button
                                id="brand-import-failures"
                                variant="outline"
                                disabled={busy || download.isPending}
                                onClick={() => download.mutate("failures")}
                            >
                                下载失败行
                            </Button>
                        )}
                        <Button
                            id="brand-import-close"
                            variant="outline"
                            disabled={busy}
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {!mutation.isSuccess && (
                            <LoadingButton
                                id="brand-import-submit"
                                loading={mutation.isPending}
                                disabled={busy || rows.length === 0}
                                onClick={() => {
                                    if (source)
                                        mutation.mutate(
                                            {
                                                requestId: source.requestId,
                                                rows,
                                                fileName: source.file.name,
                                            },
                                            {
                                                onSuccess: (job) => {
                                                    onSubmitted(
                                                        job,
                                                        source.file.name,
                                                    )
                                                    onClose()
                                                },
                                            },
                                        )
                                }}
                            >
                                {uncertain ? "重新确认" : "提交后台导入"}
                            </LoadingButton>
                        )}
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}
