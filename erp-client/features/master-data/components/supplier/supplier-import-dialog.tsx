"use client"
import { useState } from "react"
import Link from "next/link"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { apiPost } from "@/lib/api"
import { getErrorMessage, ApiErrorException } from "@/lib/api/errors"
import {
    readSupplierFile,
    downloadSupplierFailures,
    type SupplierImportRow,
    type SupplierImportResult,
} from "@/features/master-data/lib/supplier-import"

const STATUS_LABELS = {
    succeeded: "导入成功",
    skipped: "重复跳过",
    failed: "导入失败",
    uncertain: "结果待确认",
}

export const SupplierImportDialog = ({ onClose }: { onClose: () => void }) => {
    const [rows, setRows] = useState<SupplierImportRow[]>([])
    const [results, setResults] = useState<SupplierImportResult[]>([])
    const [fileName, setFileName] = useState("")
    const [error, setError] = useState("")
    const [reading, setReading] = useState(false)
    const [uncertain, setUncertain] = useState(false)
    const client = useQueryClient()
    const mutation = useMutation({
        mutationFn: (rows: SupplierImportRow[]) =>
            apiPost<SupplierImportResult[]>(
                "/admin/supplier-profiles/import",
                { rows },
                { timeoutMs: 120_000 },
            ),
        retry: false,
    })
    const locked = reading || mutation.isPending || uncertain
    const read = async (file?: File) => {
        if (!file) return
        setReading(true)
        setError("")
        setRows([])
        setResults([])
        setFileName(file.name)
        try {
            setRows(await readSupplierFile(file))
        } catch (err) {
            setError(getErrorMessage(err, "文件读取失败，请检查格式"))
        } finally {
            setReading(false)
        }
    }
    const submit = async () => {
        setError("")
        try {
            const result = await mutation.mutateAsync(rows)
            setResults(result)
            setUncertain(result.some((row) => row.status === "uncertain"))
            await client.invalidateQueries({ queryKey: ["master-data"] })
        } catch (err) {
            setUncertain(
                !(
                    err instanceof ApiErrorException &&
                    typeof err.status === "number" &&
                    err.status >= 400 &&
                    err.status < 500 &&
                    err.status !== 408
                ),
            )
            setError(
                `${getErrorMessage(err, "导入响应未收到")}。请点击重试核对，已导入的供应商不会重复创建。`,
            )
        }
    }
    const summary = results.length
        ? Object.entries(STATUS_LABELS)
              .map(
                  ([status, label]) =>
                      `${label} ${results.filter((r) => r.status === status).length} 行`,
              )
              .join("，")
        : `已读取 ${rows.length} 行，提交后逐行校验并返回结果。`
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !locked) onClose()
            }}
        >
            <DialogContent
                closeButtonId="supplier-import-dismiss"
                id="supplier-import-dialog"
                className="max-h-[90dvh] overflow-y-auto sm:max-w-4xl"
            >
                <DialogHeader>
                    <DialogTitle>导入供应商</DialogTitle>
                    <DialogDescription>
                        使用供应商信息录入模板。资料不完整的行不写入；重复供应商跳过，其余行独立导入。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-4">
                    <div className="rounded-lg bg-muted/50 p-4 text-sm leading-6">
                        签约和付款主体互相补齐，并按公司全称、简称或别名匹配。周期结算默认期末后
                        15 天计划付款；不明确的合同有效期留空。
                        <Link
                            id="supplier-import-companies"
                            className="ml-2 underline"
                            href="/master-data/companies"
                            target="_blank"
                        >
                            维护公司主体
                        </Link>
                    </div>
                    <Input
                        id="supplier-import-file"
                        aria-label="选择供应商 Excel 文件"
                        type="file"
                        accept=".xlsx"
                        disabled={locked}
                        onChange={(event) => void read(event.target.files?.[0])}
                    />
                    {fileName && (
                        <p className="text-sm">
                            {fileName} · {reading ? "读取中…" : summary}
                        </p>
                    )}
                    {error && (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    )}
                    {rows.length > 0 && (
                        <div className="max-h-80 overflow-auto rounded-lg border">
                            <table className="w-full text-left text-sm">
                                <thead className="sticky top-0 bg-muted">
                                    <tr>
                                        <th className="p-3">Excel 行</th>
                                        <th className="p-3">供应商全称</th>
                                        <th className="p-3">结果</th>
                                        <th className="p-3">说明</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.map((row) => {
                                        const result = results.find(
                                            (result) =>
                                                result.row_number ===
                                                row.row_number,
                                        )
                                        return (
                                            <tr
                                                key={row.row_number}
                                                className="border-t"
                                            >
                                                <td className="p-3">
                                                    {row.row_number}
                                                </td>
                                                <td className="min-w-48 p-3">
                                                    {row.cells[1] || "未填写"}
                                                </td>
                                                <td className="whitespace-nowrap p-3">
                                                    {result
                                                        ? STATUS_LABELS[
                                                              result.status
                                                          ]
                                                        : row.parse_errors
                                                                .length
                                                          ? "读取失败"
                                                          : "待导入"}
                                                </td>
                                                <td className="min-w-56 p-3">
                                                    {row.parse_errors.join(
                                                        "；",
                                                    ) ||
                                                        result?.message ||
                                                        "提交时核对必填数据、公司主体及重复记录"}
                                                </td>
                                            </tr>
                                        )
                                    })}
                                </tbody>
                            </table>
                        </div>
                    )}
                    <div className="flex flex-wrap justify-end gap-2">
                        {results.some((row) => row.status === "failed") && (
                            <Button
                                id="supplier-import-download-failures"
                                variant="outline"
                                onClick={() =>
                                    void downloadSupplierFailures(
                                        rows,
                                        results,
                                    ).catch((err) =>
                                        setError(
                                            getErrorMessage(
                                                err,
                                                "失败清单下载失败",
                                            ),
                                        ),
                                    )
                                }
                            >
                                下载失败行
                            </Button>
                        )}
                        <Button
                            id="supplier-import-close"
                            variant="outline"
                            disabled={locked}
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <Button
                            id="supplier-import-submit"
                            disabled={
                                !rows.length ||
                                reading ||
                                mutation.isPending ||
                                (results.length > 0 && !uncertain)
                            }
                            onClick={() => void submit()}
                        >
                            {mutation.isPending
                                ? "导入中…"
                                : uncertain
                                  ? "重试核对"
                                  : "开始导入"}
                        </Button>
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}
