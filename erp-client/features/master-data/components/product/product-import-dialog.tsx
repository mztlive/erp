"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"
import type { ProductImportJob } from "@/features/master-data/api/product-import"
import { useSubmitProductImportMutation } from "@/features/master-data/hooks/use-product-import"

export function ProductImportDialog({
    onClose,
    onSubmitted,
}: {
    onClose: () => void
    onSubmitted: (job: ProductImportJob) => void
}) {
    const [file, setFile] = useState<File | null>(null)
    const [requestId, setRequestId] = useState("")
    const [error, setError] = useState("")
    const mutation = useSubmitProductImportMutation()
    const locked = mutation.isPending

    const pick = (next?: File) => {
        setError("")
        setFile(next ?? null)
        setRequestId(crypto.randomUUID())
    }

    const submit = async () => {
        if (!file) return
        setError("")
        try {
            const job = await mutation.mutateAsync({ file, requestId })
            onSubmitted(job)
            onClose()
        } catch (err) {
            setError(
                getErrorMessage(err, "导入任务提交失败，请检查模板后重试。"),
            )
        }
    }

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
                        使用产品报价表「对内」工作表。提交后后台逐行导入，进度与结果在「后台任务」查看。图片随文件导入；供应商与成本价不会写入供给。
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
                            {file.name} ·{" "}
                            {(file.size / (1024 * 1024)).toFixed(1)} MB
                        </p>
                    ) : null}
                    {error ? (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    ) : null}
                    <div className="flex justify-end gap-2">
                        <Button
                            id="product-import-close"
                            variant="outline"
                            disabled={locked}
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <Button
                            id="product-import-submit"
                            disabled={!file || locked}
                            onClick={() => void submit()}
                        >
                            {locked ? "提交中…" : "开始导入"}
                        </Button>
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}
