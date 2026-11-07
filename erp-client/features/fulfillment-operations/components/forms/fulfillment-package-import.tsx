"use client"

import { useEffect, useRef, useState } from "react"
import { DownloadIcon, UploadIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    createPackingWorkbook,
    readPackingWorkbook,
    type PackingImportContext,
    type PackingImportPreview,
} from "../../lib/packing-import"
import type { DeliveryPacking } from "../../lib/delivery-packing"
import { downloadBlob } from "@/lib/download-blob"

type ImportProps = PackingImportContext & {
    unit: (id: string) => string
    disabled?: boolean
    onApply: (packing: DeliveryPacking) => void
}

export function FulfillmentPackageImport(props: ImportProps) {
    const [open, setOpen] = useState(false)
    return (
        <>
            <Button
                id="fulfillment-packing-import-open"
                type="button"
                variant="outline"
                size="sm"
                disabled={props.disabled}
                onClick={() => setOpen(true)}
            >
                <UploadIcon />
                Excel 批量覆盖包裹
            </Button>
            {open && (
                <PackageImportDialog
                    {...props}
                    onClose={() => setOpen(false)}
                />
            )}
        </>
    )
}

/** 文件读取只生成预览；确认时校验页面未发生变化，再一次性替换本地录入。 */
function PackageImportDialog({
    onClose,
    ...props
}: ImportProps & { onClose: () => void }) {
    const [busy, setBusy] = useState(false)
    const [error, setError] = useState("")
    const [preview, setPreview] = useState<{
        result: PackingImportPreview
        snapshot: string
        filename: string
    } | null>(null)
    const generation = useRef(0)
    useEffect(
        () => () => {
            generation.current++
        },
        [],
    )
    const snapshot = JSON.stringify([
        props.operationId,
        props.source,
        props.packing,
    ])
    const stale = preview !== null && preview.snapshot !== snapshot
    const prefix = "fulfillment-packing-import"
    const message = (reason: unknown) =>
        reason instanceof Error
            ? reason.message
            : "文件处理失败，请重新下载模板后重试"
    const download = async () => {
        const request = ++generation.current
        setBusy(true)
        setError("")
        try {
            const blob = await createPackingWorkbook(props)
            if (request !== generation.current) return
            downloadBlob(blob, "履约包裹导入模板.xlsx", {
                linkId: `${prefix}-download-file`,
                revokeDelayMs: 1000,
            })
        } catch (reason) {
            if (request === generation.current) setError(message(reason))
        } finally {
            if (request === generation.current) setBusy(false)
        }
    }
    const read = async (file: File) => {
        const request = ++generation.current
        setBusy(true)
        setError("")
        setPreview(null)
        try {
            const result = await readPackingWorkbook(file, props)
            if (request === generation.current)
                setPreview({ result, snapshot, filename: file.name })
        } catch (reason) {
            if (request === generation.current) setError(message(reason))
        } finally {
            if (request === generation.current) setBusy(false)
        }
    }
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
        >
            <DialogContent
                className="sm:max-w-3xl"
                closeButtonId={`${prefix}-close`}
            >
                <DialogHeader>
                    <DialogTitle>Excel 批量覆盖包裹</DialogTitle>
                    <DialogDescription>
                        只替换 Excel
                        中出现的地址的全部包裹，其他地址保留。收货地址和本次发货数量不变。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3">
                    <Button
                        id={`${prefix}-template`}
                        type="button"
                        variant="outline"
                        disabled={busy || props.disabled}
                        onClick={() => void download()}
                    >
                        <DownloadIcon />
                        下载当前任务模板
                    </Button>
                    <p className="text-sm text-muted-foreground">
                        修改包裹数量、物流公司和单号。不需要覆盖的地址请整行删除；拆包请复制完整商品行，同一包裹的不同商品填写相同单号。每个地址须包含本次发货的全部商品，数量合计须与页面一致。
                    </p>
                    <div className="space-y-2">
                        <Label htmlFor={`${prefix}-file`}>
                            上传填写后的 Excel
                        </Label>
                        <Input
                            id={`${prefix}-file`}
                            type="file"
                            accept=".xlsx,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                            disabled={busy || props.disabled}
                            aria-describedby={`${prefix}-help`}
                            onChange={(event) => {
                                const file = event.target.files?.[0]
                                event.target.value = ""
                                if (file) void read(file)
                            }}
                        />
                        <p
                            id={`${prefix}-help`}
                            className="text-xs text-muted-foreground"
                        >
                            仅支持 .xlsx，最多5
                            MB、100行包裹商品。物流单号须为文本格式。
                        </p>
                    </div>
                </div>
                {busy && (
                    <p role="status" className="text-sm text-muted-foreground">
                        正在处理 Excel…
                    </p>
                )}
                {error && (
                    <p role="alert" className="text-sm text-destructive">
                        {error}
                    </p>
                )}
                {stale && (
                    <p role="alert" className="text-sm text-destructive">
                        页面上的包裹或收货安排已变化，请重新上传并核对预览。
                    </p>
                )}
                {preview && !stale && (
                    <div className="min-w-0 space-y-3">
                        <p role="status" className="text-sm">
                            {preview.filename}：将覆盖{" "}
                            {preview.result.addresses.length} 个地址，共{" "}
                            {preview.result.rows} 行包裹商品；其余{" "}
                            {props.source.destinations.length -
                                preview.result.addresses.length}{" "}
                            个地址保留。
                        </p>
                        <div className="max-h-[40vh] overflow-auto rounded-md border">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>收货地址</TableHead>
                                        <TableHead>包裹变化</TableHead>
                                        <TableHead>导入包裹</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {preview.result.addresses.map((entry) => {
                                        const destination =
                                            props.source.destinations.find(
                                                (row) => row.key === entry.key,
                                            )!
                                        const parcels =
                                            preview.result.packing.packages.filter(
                                                (row) =>
                                                    row.destination_key ===
                                                    entry.key,
                                            )
                                        return (
                                            <TableRow key={entry.key}>
                                                <TableCell className="min-w-44 whitespace-normal align-top">
                                                    <p>
                                                        {destination.recipient}{" "}
                                                        · {destination.phone}
                                                    </p>
                                                    <p className="mt-1 text-xs text-muted-foreground">
                                                        {destination.address}
                                                    </p>
                                                </TableCell>
                                                <TableCell className="whitespace-nowrap align-top">
                                                    {entry.before} →{" "}
                                                    {entry.after}
                                                </TableCell>
                                                <TableCell className="min-w-52 space-y-2 whitespace-normal align-top">
                                                    {parcels.map((parcel) => (
                                                        <div key={parcel.id}>
                                                            <p className="break-all">
                                                                {parcel.carrier}{" "}
                                                                ·{" "}
                                                                {
                                                                    parcel.tracking_no
                                                                }
                                                            </p>
                                                            <p className="text-xs text-muted-foreground">
                                                                {parcel.items
                                                                    .map(
                                                                        (
                                                                            item,
                                                                        ) =>
                                                                            `${props.itemName(item.delivery_line_id)} × ${item.quantity} ${props.unit(item.delivery_line_id)}`,
                                                                    )
                                                                    .join("；")}
                                                            </p>
                                                        </div>
                                                    ))}
                                                </TableCell>
                                            </TableRow>
                                        )
                                    })}
                                </TableBody>
                            </Table>
                        </div>
                        <p className="text-xs text-muted-foreground">
                            确认覆盖后更新页面录入；请再保存草稿或确认发货。
                        </p>
                    </div>
                )}
                <DialogFooter>
                    <Button
                        id={`${prefix}-cancel`}
                        type="button"
                        variant="outline"
                        onClick={onClose}
                    >
                        取消
                    </Button>
                    <Button
                        id={`${prefix}-apply`}
                        type="button"
                        disabled={!preview || stale || busy || props.disabled}
                        onClick={() => {
                            if (!preview || stale || busy || props.disabled)
                                return
                            props.onApply(preview.result.packing)
                            onClose()
                        }}
                    >
                        确认覆盖包裹
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
