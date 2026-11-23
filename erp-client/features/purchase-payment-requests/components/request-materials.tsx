"use client"

import { useEffect, useRef, useState, type ReactNode } from "react"
import Image from "next/image"
import { FileIcon, FileTextIcon, ImageIcon, XIcon } from "lucide-react"
import {
    DetailWorkspaceSection,
    MoneyValue,
    PaperDocument,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogTitle,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    usePaymentMaterialsQuery,
    useRequestDownloadMutation,
    useRequestMaterialPreviewMutation,
} from "../hooks/queries"
import type {
    PaymentRequest,
    PurchaseSnapshot,
    RequestAttachment,
    SalesSnapshot,
} from "../types"

const PREVIEWABLE_TYPES = new Set([
    "application/pdf",
    "image/jpeg",
    "image/png",
    "image/webp",
])

function isPreviewableFile(file: RequestAttachment): boolean {
    if (file.content_type) return PREVIEWABLE_TYPES.has(file.content_type)
    return /\.(?:pdf|png|jpe?g|webp)$/i.test(file.file_name)
}

function fileKindMeta(file: RequestAttachment) {
    const type = file.content_type ?? ""
    if (type === "application/pdf" || /\.pdf$/i.test(file.file_name)) {
        return { label: "PDF", Icon: FileTextIcon }
    }
    if (
        type.startsWith("image/") ||
        /\.(?:png|jpe?g|webp)$/i.test(file.file_name)
    ) {
        return { label: "图片", Icon: ImageIcon }
    }
    return { label: "文件", Icon: FileIcon }
}

type Preview =
    | {
          kind: "purchase"
          number: string
          version: number
          snapshot: PurchaseSnapshot
      }
    | { kind: "sales"; snapshot: SalesSnapshot }

/** 预览只读取申请提交时冻结的版本，不补读当前业务单。 */
export function RequestMaterials({
    request,
    workItemId,
    submissionVersion,
    idPrefix = "ppr",
    summary = false,
    concise = false,
    action,
    detailWorkspace = false,
}: {
    request: PaymentRequest
    workItemId?: string
    submissionVersion?: number
    idPrefix?: string
    summary?: boolean
    concise?: boolean
    action?: ReactNode
    detailWorkspace?: boolean
}) {
    const query = usePaymentMaterialsQuery(
        request.id,
        workItemId,
        submissionVersion,
    )
    const download = useRequestDownloadMutation()
    const filePreview = useRequestMaterialPreviewMutation()
    const [preview, setPreview] = useState<Preview | null>(null)
    const [activeFile, setActiveFile] = useState<{
        file: RequestAttachment
        url: string | null
        contentType: string
    } | null>(null)
    const previewRequestRef = useRef(0)

    useEffect(() => {
        const url = activeFile?.url
        return () => {
            if (url) URL.revokeObjectURL(url)
        }
    }, [activeFile?.url])

    const openFilePreview = (file: RequestAttachment) => {
        const requestToken = ++previewRequestRef.current
        setActiveFile({
            file,
            url: null,
            contentType: file.content_type ?? "",
        })
        filePreview.mutate(
            {
                requestId: request.id,
                attachment: file,
                workItemId,
                submissionVersion,
            },
            {
                onSuccess: (blob) => {
                    if (previewRequestRef.current !== requestToken) return
                    const url = URL.createObjectURL(blob)
                    setActiveFile({
                        file,
                        url,
                        contentType: blob.type || file.content_type || "",
                    })
                },
            },
        )
    }
    const files = [
        ...new Map(
            [
                ...(request.attachments ?? []),
                ...(query.data?.sales.flatMap((sales) => sales.materials) ??
                    []),
            ].map((file) => [file.file_asset_id, file]),
        ).values(),
    ]
    const title = summary || concise ? "审核材料" : "采购、销售与审核材料"
    const ariaLabel = "申请提交时的采购与销售单据"
    const content = (
        <>
            {!summary && !concise && (
                <p className="text-xs text-muted-foreground">
                    {request.status === "draft"
                        ? "以下为保存草稿时核对的生效来源；提交时将再次校验并冻结。"
                        : "以下预览保留申请提交时的来源版本。"}
                    采购成本与销售成交金额分别展示。
                </p>
            )}
            {query.isPending ? (
                <p className="text-xs text-muted-foreground">
                    正在读取来源资料…
                </p>
            ) : query.isError ? (
                <div className="space-y-2">
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        {getErrorMessage(query.error, "资料读取失败")}
                    </p>
                    <Button
                        id={`${idPrefix}-materials-retry`}
                        variant={detailWorkspace ? "ghost" : "outline"}
                        size="sm"
                        className={
                            detailWorkspace
                                ? detailWorkspaceStyles.subtleButton
                                : undefined
                        }
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : !summary ? (
                <div className="space-y-4">
                    <div className="flex flex-wrap gap-2">
                        {query.data.purchases.map((purchase) => (
                            <Button
                                key={purchase.purchase_order_id}
                                id={`${idPrefix}-purchase-preview-${toAutomationIdSegment(purchase.purchase_order_id)}`}
                                variant={detailWorkspace ? "ghost" : "outline"}
                                size="sm"
                                className={
                                    detailWorkspace
                                        ? `${detailWorkspaceStyles.subtleButton} max-w-full`
                                        : "max-w-full"
                                }
                                onClick={() =>
                                    setPreview({
                                        kind: "purchase",
                                        number: purchase.purchase_order_no,
                                        version: purchase.version,
                                        snapshot: purchase.snapshot,
                                    })
                                }
                            >
                                <span className="num min-w-0 truncate">
                                    采购 {purchase.purchase_order_no} · V
                                    {purchase.version}
                                </span>
                            </Button>
                        ))}
                    </div>
                    <div className="flex flex-wrap gap-2">
                        {query.data.sales.map((sales) => (
                            <Button
                                key={sales.revision_id}
                                id={`${idPrefix}-sales-preview-${toAutomationIdSegment(sales.revision_id)}`}
                                variant={detailWorkspace ? "ghost" : "outline"}
                                size="sm"
                                className={
                                    detailWorkspace
                                        ? `${detailWorkspaceStyles.subtleButton} max-w-full`
                                        : "max-w-full"
                                }
                                onClick={() =>
                                    setPreview({
                                        kind: "sales",
                                        snapshot: sales,
                                    })
                                }
                            >
                                <span className="num min-w-0 truncate">
                                    销售 {sales.sales_order_no} · V
                                    {sales.revision_no}
                                </span>
                            </Button>
                        ))}
                    </div>
                    {query.data.purchases.length === 0 && (
                        <p className="text-xs text-muted-foreground">
                            暂无可预览的采购资料。
                        </p>
                    )}
                </div>
            ) : null}
            {files.length ? (
                <ul className="divide-y divide-border/70 rounded-lg border border-border/80">
                    {files.map((file) => {
                        const kind = fileKindMeta(file)
                        const segment = toAutomationIdSegment(
                            file.file_asset_id,
                        )
                        const downloading =
                            download.isPending &&
                            download.variables?.attachment.file_asset_id ===
                                file.file_asset_id
                        return (
                            <li
                                className="flex flex-wrap items-center gap-3 px-4 py-3 text-body-compact"
                                key={file.file_asset_id}
                            >
                                <span className="flex size-9 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
                                    <kind.Icon
                                        aria-hidden="true"
                                        className="size-4"
                                    />
                                </span>
                                <span className="min-w-0 flex-1">
                                    <span className="block break-all font-medium">
                                        {file.file_name}
                                    </span>
                                    <span className="block text-xs text-muted-foreground">
                                        {kind.label}
                                    </span>
                                </span>
                                <div className="flex shrink-0 items-center gap-2">
                                    {isPreviewableFile(file) ? (
                                        <Button
                                            id={`${idPrefix}-attachment-${segment}-preview`}
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            onClick={() =>
                                                openFilePreview(file)
                                            }
                                        >
                                            查看
                                        </Button>
                                    ) : null}
                                    <LoadingButton
                                        loading={downloading}
                                        id={`${idPrefix}-attachment-${segment}-download`}
                                        type="button"
                                        variant={
                                            detailWorkspace
                                                ? "ghost"
                                                : "outline"
                                        }
                                        size="sm"
                                        className={
                                            detailWorkspace
                                                ? `${detailWorkspaceStyles.subtleButton} shrink-0`
                                                : "shrink-0"
                                        }
                                        disabled={download.isPending}
                                        onClick={() =>
                                            download.mutate({
                                                requestId: request.id,
                                                attachment: file,
                                                workItemId,
                                                submissionVersion,
                                            })
                                        }
                                    >
                                        {downloading ? "读取中…" : "下载"}
                                    </LoadingButton>
                                </div>
                            </li>
                        )
                    })}
                </ul>
            ) : !query.isPending && !query.isError ? (
                <p className="text-xs text-muted-foreground">
                    未上传对账材料或其他附件。
                </p>
            ) : null}
            {download.isError && (
                <p role="alert" className="text-body-compact text-destructive">
                    {getErrorMessage(download.error, "附件读取失败")}
                </p>
            )}
            <Dialog
                open={Boolean(activeFile)}
                onOpenChange={(open) => {
                    if (!open) {
                        previewRequestRef.current += 1
                        setActiveFile(null)
                        filePreview.reset()
                    }
                }}
            >
                <DialogContent
                    showCloseButton={false}
                    className="flex h-[min(88dvh,54rem)] w-full max-w-[calc(100%-1.5rem)] flex-col gap-0 overflow-hidden rounded-xl border border-border/80 bg-card p-0 shadow-xl sm:max-w-5xl"
                >
                    <div className="flex shrink-0 items-center justify-between gap-4 border-b border-border/60 bg-card px-6 py-3.5">
                        <DialogTitle className="min-w-0 truncate text-section-title font-semibold tracking-tight">
                            {activeFile?.file.file_name ?? "附件预览"}
                        </DialogTitle>
                        <div className="flex shrink-0 items-center gap-2">
                            {activeFile ? (
                                <Button
                                    id={`${idPrefix}-file-preview-download`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    disabled={download.isPending}
                                    onClick={() =>
                                        download.mutate({
                                            requestId: request.id,
                                            attachment: activeFile.file,
                                            workItemId,
                                            submissionVersion,
                                        })
                                    }
                                >
                                    下载原文件
                                </Button>
                            ) : null}
                            <DialogClose
                                render={
                                    <Button
                                        id={`${idPrefix}-file-preview-close`}
                                        type="button"
                                        variant="ghost"
                                        size="icon-sm"
                                        className="text-muted-foreground hover:text-foreground"
                                    />
                                }
                            >
                                <XIcon aria-hidden="true" />
                                <span className="sr-only">关闭附件预览</span>
                            </DialogClose>
                        </div>
                    </div>
                    <DialogDescription className="sr-only">
                        付款申请提交时的冻结附件预览，按 Esc 或点击空白处关闭。
                    </DialogDescription>
                    <div className="relative min-h-0 flex-1 bg-muted/20">
                        {filePreview.isError ? (
                            <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
                                <p
                                    role="alert"
                                    className="text-body-compact text-destructive"
                                >
                                    {getErrorMessage(
                                        filePreview.error,
                                        "附件预览读取失败",
                                    )}
                                </p>
                                {activeFile ? (
                                    <Button
                                        id={`${idPrefix}-file-preview-retry`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() =>
                                            openFilePreview(activeFile.file)
                                        }
                                    >
                                        重试预览
                                    </Button>
                                ) : null}
                            </div>
                        ) : !activeFile?.url ? (
                            <div
                                role="status"
                                className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground"
                            >
                                <Spinner />
                                正在读取附件…
                            </div>
                        ) : activeFile.contentType === "application/pdf" ||
                          /\.pdf$/i.test(activeFile.file.file_name) ? (
                            <iframe
                                id={`${idPrefix}-file-preview-frame`}
                                src={activeFile.url}
                                title={activeFile.file.file_name}
                                className="h-full w-full border-0"
                            />
                        ) : (
                            <div className="relative h-full w-full p-4">
                                <Image
                                    src={activeFile.url}
                                    alt={activeFile.file.file_name}
                                    fill
                                    unoptimized
                                    sizes="90vw"
                                    className="object-contain"
                                />
                            </div>
                        )}
                    </div>
                </DialogContent>
            </Dialog>
            <Dialog
                open={Boolean(preview)}
                onOpenChange={(open) => {
                    if (!open) setPreview(null)
                }}
            >
                <DialogContent
                    showCloseButton={false}
                    className="flex max-h-[min(94dvh,56rem)] w-full max-w-[calc(100%-1.5rem)] flex-col gap-0 overflow-hidden rounded-xl border border-border/80 bg-card p-0 shadow-xl sm:max-w-5xl"
                >
                    <div className="flex shrink-0 items-center justify-between gap-4 border-b border-border/60 bg-card px-6 py-3.5">
                        <DialogTitle className="min-w-0 truncate text-section-title font-semibold tracking-tight">
                            {preview?.kind === "purchase"
                                ? "采购单完整预览"
                                : "销售单完整预览"}
                        </DialogTitle>
                        <DialogClose
                            render={
                                <Button
                                    id={`${idPrefix}-paper-close`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                    className="text-muted-foreground hover:text-foreground"
                                />
                            }
                        >
                            <XIcon aria-hidden="true" />
                            <span className="sr-only">关闭预览</span>
                        </DialogClose>
                    </div>
                    <DialogDescription className="sr-only">
                        付款申请提交时的完整单据，按 Esc 或点击空白处关闭。
                    </DialogDescription>
                    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card p-6 sm:p-8">
                        {preview?.kind === "purchase" ? (
                            <PurchasePaper preview={preview} />
                        ) : preview ? (
                            <SalesPaper
                                sales={preview.snapshot}
                                payer={request.our_company_name}
                            />
                        ) : null}
                    </div>
                </DialogContent>
            </Dialog>
        </>
    )
    return detailWorkspace ? (
        <DetailWorkspaceSection
            width="form"
            divider={false}
            title={title}
            ariaLabel={ariaLabel}
            actions={action}
        >
            {content}
        </DetailWorkspaceSection>
    ) : (
        <section
            className="space-y-4 border-t border-border/60 pt-5"
            aria-label={ariaLabel}
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <h3 className="text-body-compact font-semibold">{title}</h3>
                {action}
            </div>
            {content}
        </section>
    )
}

function PurchasePaper({
    preview,
}: {
    preview: Extract<Preview, { kind: "purchase" }>
}) {
    const snapshot = preview.snapshot
    return (
        <PaperDocument
            frame="bare"
            title="采购单"
            subtitle="采购成本"
            documentNumber={preview.number}
            version={`V${preview.version}`}
            parties={[
                {
                    id: "payer",
                    label: "我方付款主体",
                    name: snapshot.our_company_name,
                },
                {
                    id: "supplier",
                    label: "供应商",
                    name: snapshot.supplier_name,
                },
            ]}
            metadata={[
                {
                    id: "payment",
                    label: "付款条件",
                    value: snapshot.payment_term_name,
                },
            ]}
            rows={snapshot.items}
            getRowId={(row) => row.id}
            columns={[
                {
                    id: "name",
                    header: "采购项目 / 规格",
                    cell: (row) => (
                        <div>
                            {row.item_name}
                            {row.spec && (
                                <p className="text-xs text-muted-foreground">
                                    {row.spec}
                                </p>
                            )}
                        </div>
                    ),
                },
                {
                    id: "quantity",
                    header: "数量",
                    align: "end",
                    cell: (row) => (
                        <QuantityValue value={row.quantity} unit={row.unit} />
                    ),
                },
                {
                    id: "cost",
                    header: "采购含税单价",
                    align: "end",
                    cell: (row) => (
                        <MoneyValue display="exact" value={row.unit_price} />
                    ),
                },
                {
                    id: "amount",
                    header: "采购含税金额",
                    align: "end",
                    cell: (row) => <MoneyValue value={row.amount} />,
                },
            ]}
            totals={[
                {
                    id: "gross",
                    label: "采购含税总额",
                    value: <MoneyValue value={snapshot.total_amount} />,
                    emphasized: true,
                },
            ]}
        />
    )
}
function SalesPaper({ sales, payer }: { sales: SalesSnapshot; payer: string }) {
    return (
        <PaperDocument
            frame="bare"
            title="销售单"
            subtitle="客户成交金额"
            documentNumber={sales.sales_order_no}
            version={`V${sales.revision_no}`}
            parties={[
                { id: "company", label: "我方结算主体", name: payer },
                { id: "customer", label: "客户", name: sales.customer_name },
            ]}
            metadata={[
                {
                    id: "contract",
                    label: "合同",
                    value: sales.contract_no ?? "—",
                },
            ]}
            rows={sales.lines}
            getRowId={(row) => row.sales_order_revision_line_id}
            columns={[
                { id: "line", header: "序号", cell: (row) => row.line_no },
                {
                    id: "name",
                    header: "销售项目 / 规格",
                    cell: (row) => (
                        <div>
                            {row.item_name}
                            {row.specification && (
                                <p className="text-xs text-muted-foreground">
                                    {row.specification}
                                </p>
                            )}
                        </div>
                    ),
                },
                {
                    id: "quantity",
                    header: "数量",
                    align: "end",
                    cell: (row) => (
                        <QuantityValue value={row.quantity} unit={row.unit} />
                    ),
                },
                {
                    id: "price",
                    header: "销售含税单价",
                    align: "end",
                    cell: (row) => (
                        <MoneyValue
                            display="exact"
                            value={row.unit_price_gross}
                        />
                    ),
                },
                {
                    id: "gross",
                    header: "销售含税金额",
                    align: "end",
                    cell: (row) => <MoneyValue value={row.gross_amount} />,
                },
            ]}
            totals={[
                {
                    id: "gross",
                    label: "销售含税总额",
                    value: <MoneyValue value={sales.totals.gross} />,
                    emphasized: true,
                },
                {
                    id: "net",
                    label: "不含税金额",
                    value: <MoneyValue value={sales.totals.net} />,
                },
                {
                    id: "tax",
                    label: "税额",
                    value: <MoneyValue value={sales.totals.tax} />,
                },
            ]}
        />
    )
}
