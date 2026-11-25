"use client"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"

import { useMemo, useState, type ReactNode } from "react"
import {
    DetailWorkspaceMeta,
    DetailWorkspaceSection,
    MoneyValue,
    PaperDocument,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    MaterialFileList,
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fetchMaterialPreview } from "../api"
import {
    usePaymentMaterialsQuery,
    useRequestDownloadMutation,
} from "../hooks/queries"
import type { PaymentRequest, PurchaseSnapshot, SalesSnapshot } from "../types"

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
    onSelectFile,
    activeFileId,
}: {
    request: PaymentRequest
    workItemId?: string
    submissionVersion?: number
    idPrefix?: string
    summary?: boolean
    concise?: boolean
    action?: ReactNode
    detailWorkspace?: boolean
    /** 宽屏右轨对接回调：传入时由右栏展示附件，否则打开统一预览灯箱。 */
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
}) {
    const query = usePaymentMaterialsQuery(
        request.id,
        workItemId,
        submissionVersion,
    )
    const download = useRequestDownloadMutation()
    const [preview, setPreview] = useState<Preview | null>(null)
    const [dialogFileId, setDialogFileId] = useState<string | null>(null)

    const files = useMemo(
        () => [
            ...new Map(
                [
                    ...(request.attachments ?? []),
                    ...(query.data?.sales.flatMap((sales) => sales.materials) ??
                        []),
                ].map((file) => [file.file_asset_id, file]),
            ).values(),
        ],
        [request.attachments, query.data?.sales],
    )

    const materialItems = useMemo<readonly MaterialFileItem[]>(
        () =>
            files.map((file) => ({
                id: file.file_asset_id,
                name: file.file_name,
                mimeType: file.content_type ?? undefined,
                loadPreview: () =>
                    fetchMaterialPreview(
                        request.id,
                        file,
                        workItemId,
                        submissionVersion,
                    ),
                onDownload: () =>
                    download.mutateAsync({
                        requestId: request.id,
                        attachment: file,
                        workItemId,
                        submissionVersion,
                    }),
            })),
        [download, files, request.id, submissionVersion, workItemId],
    )

    const handlePreviewFile = (file: MaterialFileItem) => {
        if (onSelectFile) {
            onSelectFile(file.id)
        } else {
            setDialogFileId(file.id)
        }
    }

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
            {materialItems.length > 0 ||
            (!query.isPending && !query.isError) ? (
                <MaterialFileList
                    files={materialItems}
                    onPreview={handlePreviewFile}
                    activeFileId={activeFileId ?? dialogFileId ?? undefined}
                    downloadingFileId={
                        download.isPending
                            ? (download.variables?.attachment.file_asset_id ??
                              null)
                            : null
                    }
                    emptyMessage="未上传对账材料或其他附件。"
                    idPrefix={`${idPrefix}-attachment`}
                    downloadVariant={detailWorkspace ? "ghost" : "outline"}
                    downloadButtonClassName={
                        detailWorkspace
                            ? detailWorkspaceStyles.subtleButton
                            : undefined
                    }
                />
            ) : null}
            {!onSelectFile ? (
                <UnifiedMaterialViewer
                    mode="dialog"
                    files={materialItems}
                    activeFileId={dialogFileId ?? undefined}
                    onActiveFileChange={setDialogFileId}
                    open={dialogFileId !== null}
                    onOpenChange={(open) => {
                        if (!open) setDialogFileId(null)
                    }}
                    idPrefix={`${idPrefix}-file-preview`}
                />
            ) : null}
            <Dialog
                open={Boolean(preview)}
                onOpenChange={(open) => {
                    if (!open) setPreview(null)
                }}
            >
                <DialogContent
                    size="2xl"
                    scroll="body"
                    closeButtonId={`${idPrefix}-paper-close`}
                    className="bg-card"
                >
                    <DialogHeader className="border-border/60 bg-card py-3 text-left print:hidden">
                        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                            <div className="min-w-0 flex-1 space-y-1">
                                <div className="flex flex-wrap items-center gap-2">
                                    <DialogTitle className="min-w-0 truncate">
                                        {preview?.kind === "purchase"
                                            ? "采购单完整预览"
                                            : "销售单完整预览"}
                                    </DialogTitle>
                                    {preview ? (
                                        <Badge
                                            variant="neutral"
                                            className="num"
                                        >
                                            V
                                            {preview.kind === "purchase"
                                                ? preview.version
                                                : preview.snapshot.revision_no}
                                        </Badge>
                                    ) : null}
                                </div>
                                <DialogDescription className="sr-only">
                                    付款申请提交时的完整单据，按 Esc
                                    或点击空白处关闭。
                                </DialogDescription>
                                {preview ? (
                                    <DetailWorkspaceMeta
                                        items={
                                            preview.kind === "purchase"
                                                ? [
                                                      <span
                                                          key="no"
                                                          className="num"
                                                      >
                                                          {preview.number}
                                                      </span>,
                                                      preview.snapshot
                                                          .supplier_name,
                                                      preview.snapshot
                                                          .payment_term_name,
                                                  ]
                                                : [
                                                      <span
                                                          key="no"
                                                          className="num"
                                                      >
                                                          {
                                                              preview.snapshot
                                                                  .sales_order_no
                                                          }
                                                      </span>,
                                                      preview.snapshot
                                                          .customer_name,
                                                      preview.snapshot
                                                          .contract_no ??
                                                          "无合同",
                                                  ]
                                        }
                                    />
                                ) : null}
                            </div>
                            {preview ? (
                                <div className="shrink-0 text-left sm:text-right">
                                    <div className="text-xs text-muted-foreground">
                                        {preview.kind === "purchase"
                                            ? "采购含税总额"
                                            : "销售含税总额"}
                                    </div>
                                    <MoneyValue
                                        display="exact"
                                        value={
                                            preview.kind === "purchase"
                                                ? preview.snapshot.total_amount
                                                : preview.snapshot.totals.gross
                                        }
                                        className="text-value-compact font-semibold tracking-tight text-foreground"
                                    />
                                </div>
                            ) : null}
                        </div>
                    </DialogHeader>
                    <DialogBody className="bg-card px-5 py-4">
                        {preview?.kind === "purchase" ? (
                            <PurchasePaper preview={preview} />
                        ) : preview ? (
                            <SalesPaper
                                sales={preview.snapshot}
                                payer={request.our_company_name}
                            />
                        ) : null}
                    </DialogBody>
                    <DialogFooter className="items-center border-border/60 bg-card py-2.5 sm:justify-between print:hidden">
                        <p className="text-xs text-muted-foreground">
                            付款申请提交时冻结的单据版本
                        </p>
                        <Button
                            id={`${idPrefix}-paper-footer-close`}
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => setPreview(null)}
                        >
                            关闭
                        </Button>
                    </DialogFooter>
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
    const { names } = useProductUnitNames([], snapshot.items.length > 0)
    return (
        <PaperDocument
            frame="borderless"
            compact
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
                {
                    id: "lines",
                    label: "明细行数",
                    value: `${snapshot.items.length} 项`,
                    numeric: true,
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
                            <div className="font-medium text-foreground">
                                {row.item_name}
                            </div>
                            {row.spec && (
                                <p className="mt-0.5 text-xs text-muted-foreground">
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
                    numeric: true,
                    cell: (row) => (
                        <QuantityValue
                            value={row.quantity}
                            unit={names.get(row.unit?.trim() ?? "")}
                        />
                    ),
                },
                {
                    id: "cost",
                    header: "采购含税单价",
                    align: "end",
                    numeric: true,
                    cell: (row) => (
                        <MoneyValue display="exact" value={row.unit_price} />
                    ),
                },
                {
                    id: "amount",
                    header: "采购含税金额",
                    align: "end",
                    numeric: true,
                    cell: (row) => (
                        <MoneyValue display="exact" value={row.amount} />
                    ),
                },
            ]}
            totals={[
                {
                    id: "gross",
                    label: "采购含税总额",
                    value: (
                        <MoneyValue
                            display="exact"
                            value={snapshot.total_amount}
                        />
                    ),
                    emphasized: true,
                },
            ]}
        />
    )
}

function SalesPaper({ sales, payer }: { sales: SalesSnapshot; payer: string }) {
    const { names } = useProductUnitNames([], sales.lines.length > 0)
    return (
        <PaperDocument
            frame="borderless"
            compact
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
                    numeric: true,
                },
                {
                    id: "lines",
                    label: "明细行数",
                    value: `${sales.lines.length} 项`,
                    numeric: true,
                },
            ]}
            rows={sales.lines}
            getRowId={(row) => row.sales_order_revision_line_id}
            columns={[
                {
                    id: "line",
                    header: "序号",
                    numeric: true,
                    cell: (row) => row.line_no,
                },
                {
                    id: "name",
                    header: "销售项目 / 规格",
                    cell: (row) => (
                        <div>
                            <div className="font-medium text-foreground">
                                {row.item_name}
                            </div>
                            {row.specification && (
                                <p className="mt-0.5 text-xs text-muted-foreground">
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
                    numeric: true,
                    cell: (row) => (
                        <QuantityValue
                            value={row.quantity}
                            unit={names.get(row.unit?.trim() ?? "")}
                        />
                    ),
                },
                {
                    id: "price",
                    header: "销售含税单价",
                    align: "end",
                    numeric: true,
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
                    numeric: true,
                    cell: (row) => (
                        <MoneyValue display="exact" value={row.gross_amount} />
                    ),
                },
            ]}
            totals={[
                {
                    id: "net",
                    label: "不含税金额",
                    value: (
                        <MoneyValue display="exact" value={sales.totals.net} />
                    ),
                },
                {
                    id: "tax",
                    label: "税额",
                    value: (
                        <MoneyValue display="exact" value={sales.totals.tax} />
                    ),
                },
                {
                    id: "gross",
                    label: "销售含税总额",
                    value: (
                        <MoneyValue
                            display="exact"
                            value={sales.totals.gross}
                        />
                    ),
                    emphasized: true,
                },
            ]}
        />
    )
}
