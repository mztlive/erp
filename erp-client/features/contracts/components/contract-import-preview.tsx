"use client"

import { FileTextIcon, ExternalLinkIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { getErrorMessage } from "@/lib/api/errors"
import type { ContractImportTask } from "../api/upload"
import { useImportPreview } from "../hooks/use-import-preview"
import { ContractPdfPages } from "./contract-pdf-pages"

export function ContractImportPreview({ task }: { task: ContractImportTask }) {
    const preview = useImportPreview(task.id)
    return (
        <aside
            aria-label="合同原文"
            className="flex h-[560px] min-w-0 flex-col gap-4 rounded-lg bg-muted/40 p-4 lg:h-auto lg:min-h-0"
        >
            <div className="flex min-w-0 items-start gap-3">
                <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-muted">
                    <FileTextIcon className="size-5" aria-hidden="true" />
                </div>
                <div className="min-w-0 flex-1">
                    <p className="break-words text-sm font-medium">
                        {task.file_name}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        PDF · {task.page_count} 页
                    </p>
                </div>
                {preview.url ? (
                    <Button
                        id="contract-import-preview-full"
                        variant="link"
                        size="xs"
                        className="shrink-0 px-0"
                        render={
                            <a
                                aria-label="查看完整原文（新窗口）"
                                href={preview.url}
                                target="_blank"
                                rel="noopener noreferrer"
                            />
                        }
                    >
                        查看完整原文 <ExternalLinkIcon data-icon="inline-end" />
                    </Button>
                ) : null}
            </div>
            {preview.url ? (
                <>
                    {preview.data ? (
                        <ContractPdfPages blob={preview.data} />
                    ) : null}
                </>
            ) : preview.isError ? (
                <div className="flex flex-1 flex-col items-center justify-center gap-3 p-4 text-center">
                    <p className="text-sm text-muted-foreground" role="alert">
                        {getErrorMessage(preview.error, "原文加载失败，请重试")}
                    </p>
                    <Button
                        id="contract-import-preview-retry"
                        type="button"
                        variant="outline"
                        disabled={preview.isFetching}
                        onClick={() => void preview.refetch()}
                    >
                        {preview.isFetching ? <Spinner /> : null}重新加载原文
                    </Button>
                </div>
            ) : (
                <p
                    className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground"
                    role="status"
                >
                    <Spinner />
                    正在加载合同原文…
                </p>
            )}
        </aside>
    )
}
