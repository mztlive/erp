"use client"

import * as React from "react"
import { DownloadIcon } from "lucide-react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import {
    useDownloadSupplierQuotes,
    useSupplierQuoteSource,
} from "../hooks/use-supplier-quote-export"
import type { SupplierQuoteRequest } from "../api/supplier-quote-export"
import { selectedSupplierQuotes } from "../lib/supplier-quote-export"
import { SupplierQuoteSelection } from "./supplier-quote-selection"

export function SupplierQuoteExportButton({
    request,
    userId,
    onRefresh,
}: {
    request: SupplierQuoteRequest
    userId: string
    onRefresh: () => void
}) {
    const [open, setOpen] = React.useState(false)
    const [busy, setBusy] = React.useState(false)
    return (
        <>
            <Button
                id="procurement-inquiry-export-supplier-quotes"
                variant="outline"
                type="button"
                onClick={() => setOpen(true)}
            >
                <DownloadIcon aria-hidden="true" data-icon="inline-start" />
                生成供应商报价表
            </Button>
            <Dialog
                open={open}
                onOpenChange={(value) => {
                    if (!busy) setOpen(value)
                }}
            >
                <DialogContent
                    id="supplier-quote-export-dialog"
                    closeButtonId="supplier-quote-export-close"
                    className="max-h-[90vh] gap-4 overflow-y-auto sm:max-w-4xl"
                >
                    <DialogHeader>
                        <DialogTitle>生成供应商报价表</DialogTitle>
                        <DialogDescription>
                            按供给关系拆分报价表，打包下载后分别发送给供应商。
                        </DialogDescription>
                    </DialogHeader>
                    {open && (
                        <SupplierQuoteExportContent
                            request={request}
                            userId={userId}
                            onRefresh={onRefresh}
                            onBusyChange={setBusy}
                            onClose={() => setOpen(false)}
                        />
                    )}
                </DialogContent>
            </Dialog>
        </>
    )
}

function SupplierQuoteExportContent({
    request,
    userId,
    onRefresh,
    onBusyChange,
    onClose,
}: {
    request: SupplierQuoteRequest
    userId: string
    onRefresh: () => void
    onBusyChange: (busy: boolean) => void
    onClose: () => void
}) {
    const query = useSupplierQuoteSource(request, userId)
    const download = useDownloadSupplierQuotes(request)
    const [excludedLines, setExcludedLines] = React.useState<string[]>([])
    const [excludedSuppliers, setExcludedSuppliers] = React.useState<string[]>(
        [],
    )
    React.useEffect(() => {
        onBusyChange(download.isPending)
        return () => onBusyChange(false)
    }, [download.isPending, onBusyChange])
    const source = query.data
    const lineIds =
        source?.lines
            .filter((line) => !excludedLines.includes(line.lineId))
            .map((line) => line.lineId) ?? []
    const supplierIds =
        source?.groups
            .filter((group) => !excludedSuppliers.includes(group.supplierId))
            .map((group) => group.supplierId) ?? []
    const groups = source
        ? selectedSupplierQuotes(source, lineIds, supplierIds)
        : []
    const covered = new Set(
        groups.flatMap((group) => group.lines.map((line) => line.lineId)),
    )
    const missing = lineIds.filter((id) => !covered.has(id)).length
    const invalid =
        source?.lines.filter(
            (line) => lineIds.includes(line.lineId) && line.issue,
        ).length ?? 0
    const disabled =
        download.isPending ||
        query.isFetching ||
        query.isError ||
        !groups.length ||
        missing > 0 ||
        invalid > 0 ||
        groups.length > 200
    const refresh = () => {
        download.reset()
        onRefresh()
        void query.refetch()
    }
    return (
        <>
            {query.isPending && (
                <p role="status" className="py-8 text-sm text-muted-foreground">
                    正在读取当前商品和供给关系…
                </p>
            )}
            {query.isError && (
                <BusinessFailureState
                    id="supplier-quote-source-retry"
                    title="报价表资料读取失败"
                    error={query.error}
                    onRetry={refresh}
                />
            )}
            {source && (
                <>
                    <SupplierQuoteSelection
                        source={source}
                        lineIds={lineIds}
                        supplierIds={supplierIds}
                        disabled={download.isPending || query.isFetching}
                        onLinesChange={(ids) => {
                            download.reset()
                            setExcludedLines(
                                source.lines
                                    .filter(
                                        (line) => !ids.includes(line.lineId),
                                    )
                                    .map((line) => line.lineId),
                            )
                        }}
                        onSuppliersChange={(ids) => {
                            download.reset()
                            setExcludedSuppliers(
                                source.groups
                                    .filter(
                                        (group) =>
                                            !ids.includes(group.supplierId),
                                    )
                                    .map((group) => group.supplierId),
                            )
                        }}
                    />
                    <p className="text-xs text-muted-foreground">
                        将生成{" "}
                        <span className="font-medium text-foreground">
                            {groups.length} 张报价表
                        </span>
                        ，每家均按完整需求数量询价，黄色区域供供应商填写。
                    </p>
                    {(missing > 0 || invalid > 0 || groups.length > 200) && (
                        <Alert variant="warning">
                            <AlertDescription>
                                {missing > 0 &&
                                    `${missing} 条所选商品没有匹配到已选供应商，请补齐供给关系、勾选供应商或取消相应商品。`}
                                {invalid > 0 &&
                                    `${invalid} 条所选商品资料不完整，请处理商品提示或取消勾选。`}
                                {groups.length > 200 &&
                                    "一次最多生成 200 家供应商的报价表，请减少所选供应商。"}
                            </AlertDescription>
                        </Alert>
                    )}
                </>
            )}
            {download.isError && (
                <BusinessFailureState
                    id="supplier-quote-download-retry"
                    title="报价表生成失败，本次未下载"
                    error={download.error}
                    onRetry={refresh}
                    retryLabel="刷新预览"
                />
            )}
            {download.isSuccess && (
                <p role="status" className="text-sm">
                    已生成 {download.data} 张供应商报价表并开始下载 ZIP。
                </p>
            )}
            <DialogFooter className="border-t pt-4 sm:items-center">
                <Button
                    id="supplier-quote-preview-refresh"
                    type="button"
                    variant="ghost"
                    className="sm:mr-auto"
                    disabled={download.isPending || query.isFetching}
                    onClick={refresh}
                >
                    刷新预览
                </Button>
                <Button
                    id="supplier-quote-export-cancel"
                    type="button"
                    variant="outline"
                    disabled={download.isPending}
                    onClick={onClose}
                >
                    关闭
                </Button>
                <Button
                    id="supplier-quote-export-download"
                    type="button"
                    disabled={disabled}
                    onClick={() => {
                        if (source)
                            download.mutate({ source, lineIds, supplierIds })
                    }}
                >
                    {download.isPending ? "正在生成 ZIP…" : "下载 ZIP"}
                </Button>
            </DialogFooter>
        </>
    )
}
