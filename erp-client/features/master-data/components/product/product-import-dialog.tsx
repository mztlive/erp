"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"
import type { ProductImportJob } from "@/features/master-data/api/product-import"
import { isDirectUploadCancelled } from "@/features/master-data/api/product-import"
import { useSubmitProductImportMutation } from "@/features/master-data/hooks/use-product-import"
import {
    formatUploadMegabytes,
    useProductImportDirectUpload,
} from "@/features/master-data/hooks/use-product-import-upload"

export function ProductImportDialog({
    onClose,
    onSubmitted,
}: {
    onClose: () => void
    onSubmitted: (job: ProductImportJob) => void
}) {
    const [file, setFile] = useState<File | null>(null)
    const [requestId, setRequestId] = useState("")
    const [legacyError, setLegacyError] = useState("")
    const [legacyFallback, setLegacyFallback] = useState(false)
    const legacy = useSubmitProductImportMutation()
    const direct = useProductImportDirectUpload()
    const locked = legacy.isPending || direct.active

    const pick = (next?: File) => {
        setLegacyError("")
        setLegacyFallback(false)
        direct.reset()
        setFile(next ?? null)
        setRequestId(crypto.randomUUID())
    }

    const submitDirect = async () => {
        if (!file) return
        setLegacyError("")
        setLegacyFallback(false)
        try {
            const job = await direct.start(file, requestId)
            onSubmitted(job)
            onClose()
        } catch (err) {
            // 取消上传静默收尾；直传失败展示重试与普通上传兜底。
            if (!isDirectUploadCancelled(err)) setLegacyFallback(true)
        }
    }

    const submitLegacy = async () => {
        if (!file) return
        setLegacyError("")
        try {
            const job = await legacy.mutateAsync({ file, requestId })
            onSubmitted(job)
            onClose()
        } catch (err) {
            setLegacyError(
                getErrorMessage(err, "导入任务提交失败，请检查模板后重试。"),
            )
        }
    }

    const progress = direct.progress
    const committing = progress?.phase === "committing"

    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !locked) onClose()
            }}
        >
            <DialogContent
                closeButtonId="product-import-dismiss"
                id="product-import-dialog"
                className="sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>导入商品</DialogTitle>
                    <DialogDescription>
                        使用产品报价表「对内」工作表。大文件分片直传对象存储并显示进度；提交后后台逐行导入，进度与结果在「后台任务」查看。图片随文件导入；供应商与成本价不会写入供给。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-4">
                    <Input
                        id="product-import-file"
                        aria-label="选择产品报价表"
                        type="file"
                        accept=".xlsx"
                        disabled={locked}
                        onChange={(event) => pick(event.target.files?.[0])}
                    />
                    {file ? (
                        <p className="text-sm">
                            {file.name} · {formatUploadMegabytes(file.size)}
                        </p>
                    ) : null}
                    {progress ? (
                        <div className="space-y-2">
                            <Progress
                                id="product-import-progress"
                                aria-label={
                                    committing
                                        ? "正在登记导入任务"
                                        : "正在上传导入文件"
                                }
                                value={progress.percent}
                            />
                            <p
                                className="text-sm text-muted-foreground"
                                role="status"
                            >
                                {committing
                                    ? "上传完成，正在登记导入任务…"
                                    : `正在上传第 ${progress.partIndex + 1}/${progress.totalParts} 片 · ${formatUploadMegabytes(progress.loadedBytes)} / ${formatUploadMegabytes(progress.totalBytes)} · ${progress.percent}%`}
                            </p>
                        </div>
                    ) : null}
                    {direct.error ? (
                        <p role="alert" className="text-sm text-destructive">
                            {direct.error}
                        </p>
                    ) : null}
                    {legacyError ? (
                        <p role="alert" className="text-sm text-destructive">
                            {legacyError}
                        </p>
                    ) : null}
                    <div className="flex justify-end gap-2">
                        {progress && !committing ? (
                            <Button
                                id="product-import-cancel"
                                variant="outline"
                                onClick={direct.cancel}
                            >
                                取消上传
                            </Button>
                        ) : (
                            <Button
                                id="product-import-close"
                                variant="outline"
                                disabled={locked}
                                onClick={onClose}
                            >
                                关闭
                            </Button>
                        )}
                        {legacyFallback && !direct.active ? (
                            <>
                                <Button
                                    id="product-import-retry"
                                    variant="outline"
                                    disabled={!file || locked}
                                    onClick={() => void submitDirect()}
                                >
                                    重试
                                </Button>
                                <Button
                                    id="product-import-legacy"
                                    disabled={!file || locked}
                                    onClick={() => void submitLegacy()}
                                >
                                    {legacy.isPending
                                        ? "提交中…"
                                        : "改用普通上传"}
                                </Button>
                            </>
                        ) : (
                            <Button
                                id="product-import-submit"
                                disabled={!file || locked}
                                onClick={() => void submitDirect()}
                            >
                                {direct.active
                                    ? committing
                                        ? "登记中…"
                                        : "上传中…"
                                    : "开始导入"}
                            </Button>
                        )}
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}
