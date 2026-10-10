"use client"

import { useEffect, useState } from "react"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { getErrorMessage } from "@/lib/api/errors"
import { useContractFilePreviewQuery } from "../hooks/queries"
import { ContractPdfPages } from "./contract-pdf-pages"

export function ContractFilePreviewDialog({
    contractId,
    fileId,
    fileName,
    open,
    onOpenChange,
}: {
    contractId: string
    fileId: string
    fileName: string
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const query = useContractFilePreviewQuery(contractId, fileId, open)
    const [source, setSource] = useState<{ blob: Blob; url: string }>()
    useEffect(() => {
        if (!open || !query.data) return
        const url = URL.createObjectURL(query.data)
        setSource({ blob: query.data, url })
        return () => URL.revokeObjectURL(url)
    }, [open, query.data])
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                id="sales-contract-original-dialog"
                closeButtonId="sales-contract-original-close"
                className="sm:max-w-4xl"
            >
                <DialogHeader>
                    <DialogTitle>合同原文</DialogTitle>
                    <DialogDescription>{fileName}</DialogDescription>
                </DialogHeader>
                {query.isError ? (
                    <div role="alert" className="space-y-3 py-6 text-sm">
                        <p>
                            {getErrorMessage(
                                query.error,
                                "原文加载失败，请重试",
                            )}
                        </p>
                        <Button
                            id="sales-contract-original-retry"
                            variant="outline"
                            disabled={query.isFetching}
                            onClick={() => void query.refetch()}
                        >
                            重新加载原文
                        </Button>
                    </div>
                ) : query.data ? (
                    <div className="flex h-[65vh] min-h-0 flex-col gap-3">
                        {source?.blob === query.data ? (
                            <Button
                                id="sales-contract-original-full"
                                variant="link"
                                size="sm"
                                className="self-end"
                                render={
                                    <a
                                        aria-label="查看完整合同原文（新窗口）"
                                        href={source.url}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                    />
                                }
                            >
                                查看完整原文
                            </Button>
                        ) : null}
                        <ContractPdfPages
                            blob={query.data}
                            idPrefix="sales-contract-original-pdf"
                        />
                    </div>
                ) : (
                    <div
                        role="status"
                        className="flex items-center gap-2 py-8 text-sm"
                    >
                        <Spinner />
                        正在加载合同原文…
                    </div>
                )}
            </DialogContent>
        </Dialog>
    )
}
