"use client"

import * as React from "react"

import { Separator } from "@/components/ui/separator"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { taxAmountToneClass } from "@/components/business/values"
import { documentText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

type PaperDocumentStatus = Readonly<{
    label: string
    tone?: StatusTone
}>

type PaperDocumentField = Readonly<{
    id: React.Key
    label: React.ReactNode
    value: React.ReactNode
    numeric?: boolean
}>

type PaperDocumentParty = Readonly<{
    id: React.Key
    label: React.ReactNode
    name: React.ReactNode
    reference?: React.ReactNode
    fields?: readonly PaperDocumentField[]
}>

type PaperDocumentColumnAlignment = "start" | "center" | "end"

type PaperDocumentColumn<Row> = Readonly<{
    id: string
    header: React.ReactNode
    cell: (row: Row, rowIndex: number) => React.ReactNode
    align?: PaperDocumentColumnAlignment
    numeric?: boolean
}>

type PaperDocumentTotal = Readonly<{
    id: React.Key
    label: React.ReactNode
    value: React.ReactNode
    description?: React.ReactNode
    emphasized?: boolean
}>

type PaperDocumentFrame = "framed" | "bare" | "borderless"

interface PaperDocumentProps<Row> extends Omit<
    React.ComponentProps<"article">,
    "children" | "title"
> {
    /** 出具方名称或品牌；仅展示，不推导签约主体。 */
    issuer?: React.ReactNode
    title: React.ReactNode
    subtitle?: React.ReactNode
    documentNumber: React.ReactNode
    status?: PaperDocumentStatus
    version?: React.ReactNode
    parties: readonly [PaperDocumentParty, PaperDocumentParty]
    metadata: readonly PaperDocumentField[]
    lineItemLabel?: string
    columns: readonly PaperDocumentColumn<Row>[]
    rows: readonly Row[]
    getRowId: (row: Row, rowIndex: number) => React.Key
    emptyContent?: React.ReactNode
    totals: readonly PaperDocumentTotal[]
    afterLineItems?: React.ReactNode
    remarks?: React.ReactNode
    signature?: React.ReactNode
    seal?: React.ReactNode
    /** 置于金额汇总、备注及签章之后的附表区（如多地址交付安排）。 */
    appendix?: React.ReactNode
    footer?: React.ReactNode
    /**
     * framed：浅底外框 + 内边距，适合页面内嵌。
     * bare：仅纸张本体，适合透明浮层预览，避免再套一层灰框。
     * borderless：单据内容直接铺在预览容器中，无纸张描边或阴影。
     */
    frame?: PaperDocumentFrame
    /** 弹窗预览等紧凑容器内使用，收紧页边留白（`px-6 py-6 min-h-0`）。 */
    compact?: boolean
}

const alignmentClasses = {
    start: "text-left",
    center: "text-center",
    end: "text-right",
} satisfies Record<PaperDocumentColumnAlignment, string>

/**
 * 通用纸质单据投影。
 *
 * 组件只排版调用方传入的业务记录，不计算金额、税额、大小写金额、状态或签章结果。
 * 外层在窄屏保留纸张宽度并横向滚动；article/table/thead/footer 提供打印语义。
 */
function PaperDocument<Row>({
    issuer,
    title,
    subtitle,
    documentNumber,
    status,
    version,
    parties,
    metadata,
    lineItemLabel = "单据明细",
    columns,
    rows,
    getRowId,
    emptyContent = "暂无明细",
    totals,
    afterLineItems,
    remarks,
    signature,
    seal,
    appendix,
    footer,
    frame = "framed",
    compact = false,
    className,
    "aria-label": ariaLabel,
    ...props
}: PaperDocumentProps<Row>) {
    if (compact) {
        const metadataColsClass =
            metadata.length <= 1
                ? "grid-cols-1"
                : metadata.length === 2
                  ? "grid-cols-2"
                  : metadata.length === 3
                    ? "grid-cols-2 sm:grid-cols-3"
                    : metadata.length === 4
                      ? "grid-cols-2 sm:grid-cols-4"
                      : "grid-cols-2 sm:grid-cols-3 lg:grid-cols-5"

        const totalsSection = (
            <section
                aria-label="金额汇总"
                className="w-full min-w-0 break-inside-avoid md:max-w-sm"
            >
                <dl className="divide-y divide-border/50 rounded-lg border border-border/60 bg-card px-3.5">
                    {totals.map((total) => (
                        <div
                            key={total.id}
                            className={cn(
                                "flex items-baseline justify-between gap-4",
                                total.emphasized ? "py-2.5" : "py-2",
                            )}
                        >
                            <dt
                                className={cn(
                                    "min-w-0",
                                    total.emphasized
                                        ? "text-body-compact font-medium text-foreground"
                                        : "text-xs text-muted-foreground",
                                )}
                            >
                                {total.label}
                                {total.description != null ? (
                                    <span className="mt-0.5 block text-xs font-normal text-muted-foreground">
                                        {total.description}
                                    </span>
                                ) : null}
                            </dt>
                            <dd
                                className={cn(
                                    "num shrink-0 text-right",
                                    total.emphasized
                                        ? "text-value-compact font-semibold tracking-tight"
                                        : "text-body-compact font-medium",
                                    taxAmountToneClass(total.label),
                                )}
                            >
                                {total.value}
                            </dd>
                        </div>
                    ))}
                </dl>
            </section>
        )

        return (
            <div
                data-slot="paper-document-viewport"
                data-frame={frame}
                className={cn(
                    "w-full min-w-0 overflow-x-visible print:overflow-visible print:border-0 print:bg-transparent print:p-0",
                    frame === "framed" &&
                        "rounded-lg border border-border bg-surface-sunken p-3 sm:p-4",
                    frame !== "framed" && "bg-transparent p-0",
                )}
            >
                <article
                    data-slot="paper-document"
                    aria-label={ariaLabel ?? "纸质单据预览"}
                    className={cn(
                        "text-body-compact text-card-foreground print:min-h-0 print:min-w-0 print:max-w-none print:border-0 print:px-0 print:py-0 print:shadow-none",
                        frame === "borderless"
                            ? "w-full min-w-0 max-w-none p-0"
                            : "mx-auto min-w-2xl max-w-5xl px-6 py-5",
                        frame !== "borderless" &&
                            "border border-border bg-card",
                        frame === "framed" && "shadow-lg",
                        frame === "bare" && "shadow-2xl",
                        className,
                    )}
                    {...props}
                >
                    <header className="break-inside-avoid border-b border-border/60 pb-3.5">
                        <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
                            <div className="min-w-0">
                                <div className="flex flex-wrap items-center gap-2">
                                    <h1 className="font-heading text-page-title font-semibold tracking-tight text-foreground">
                                        {title}
                                    </h1>
                                    {subtitle != null ? (
                                        <span className="rounded-sm bg-neutral-soft px-1.5 py-0.5 text-xs font-medium text-neutral-soft-foreground">
                                            {subtitle}
                                        </span>
                                    ) : null}
                                    {status != null ? (
                                        <StatusBadge
                                            tone={status.tone}
                                            label={status.label}
                                            className="print:border-border print:bg-transparent print:text-foreground"
                                        />
                                    ) : null}
                                </div>
                                {issuer != null ? (
                                    <div className="mt-1 text-xs text-muted-foreground">
                                        {issuer}
                                    </div>
                                ) : null}
                            </div>
                            <dl className="flex flex-wrap items-baseline gap-x-5 gap-y-1 pt-0.5 text-xs">
                                <div className="flex items-baseline gap-1.5">
                                    <dt className="text-muted-foreground">
                                        单据编号
                                    </dt>
                                    <dd className="num font-medium text-foreground">
                                        {documentNumber}
                                    </dd>
                                </div>
                                {version != null ? (
                                    <div className="flex items-baseline gap-1.5">
                                        <dt className="text-muted-foreground">
                                            版本
                                        </dt>
                                        <dd className="num font-medium text-foreground">
                                            {version}
                                        </dd>
                                    </div>
                                ) : null}
                            </dl>
                        </div>
                    </header>

                    <section
                        aria-label="交易双方"
                        className="mt-3.5 break-inside-avoid"
                    >
                        <div className="grid grid-cols-1 divide-y divide-border/60 rounded-lg border border-border/60 bg-surface-sunken/30 sm:grid-cols-2 sm:divide-x sm:divide-y-0">
                            {parties.map((party) => (
                                <div
                                    key={party.id}
                                    className="min-w-0 px-4 py-3"
                                >
                                    <div className="flex items-baseline justify-between gap-3">
                                        <h2 className="text-xs font-medium text-muted-foreground">
                                            {party.label}
                                        </h2>
                                        {party.reference != null ? (
                                            <div className="num min-w-0 truncate text-xs text-muted-foreground">
                                                {party.reference}
                                            </div>
                                        ) : null}
                                    </div>
                                    <div className="mt-1 font-heading text-section-title font-semibold tracking-tight text-foreground">
                                        {party.name}
                                    </div>
                                    {party.fields != null &&
                                    party.fields.length > 0 ? (
                                        <dl className="mt-2.5 grid grid-cols-2 gap-x-4 gap-y-1.5 border-t border-border/50 pt-2">
                                            {party.fields.map((field) => (
                                                <div
                                                    key={field.id}
                                                    className="min-w-0"
                                                >
                                                    <dt className="text-xs text-muted-foreground">
                                                        {field.label}
                                                    </dt>
                                                    <dd
                                                        className={cn(
                                                            "mt-0.5 break-words text-body-compact text-foreground",
                                                            field.numeric &&
                                                                "num",
                                                        )}
                                                    >
                                                        {field.value}
                                                    </dd>
                                                </div>
                                            ))}
                                        </dl>
                                    ) : null}
                                </div>
                            ))}
                        </div>
                    </section>

                    {metadata.length > 0 ? (
                        <section
                            aria-label="单据信息"
                            className="mt-3 break-inside-avoid"
                        >
                            <dl
                                className={cn(
                                    "grid gap-x-5 gap-y-2.5 rounded-lg border border-border/60 bg-card px-4 py-2.5",
                                    metadataColsClass,
                                )}
                            >
                                {metadata.map((field) => (
                                    <div key={field.id} className="min-w-0">
                                        <dt className="text-xs text-muted-foreground">
                                            {field.label}
                                        </dt>
                                        <dd
                                            className={cn(
                                                "mt-0.5 text-body-compact font-medium text-foreground",
                                                field.numeric && "num",
                                            )}
                                        >
                                            {field.value}
                                        </dd>
                                    </div>
                                ))}
                            </dl>
                        </section>
                    ) : null}

                    <section aria-label={lineItemLabel} className="mt-4">
                        <div className="mb-2 flex items-baseline justify-between gap-2">
                            <h2 className="text-body-compact font-semibold text-foreground">
                                {lineItemLabel}
                            </h2>
                            {rows.length > 0 &&
                            !lineItemLabel.includes("共") ? (
                                <span className="num text-xs text-muted-foreground">
                                    共 {rows.length} 项
                                </span>
                            ) : null}
                        </div>
                        <div className="overflow-x-auto border-y border-border/70 bg-card">
                            <Table
                                data-density="compact"
                                className="min-w-full"
                            >
                                <TableHeader className="bg-table-header/60 print:table-header-group">
                                    <TableRow className="hover:bg-transparent">
                                        {columns.map((column) => (
                                            <TableHead
                                                key={column.id}
                                                scope="col"
                                                className={cn(
                                                    "px-2.5 py-2 text-xs font-medium text-muted-foreground first:ps-2 last:pe-2",
                                                    alignmentClasses[
                                                        column.align ?? "start"
                                                    ],
                                                    column.numeric && "num",
                                                )}
                                            >
                                                {column.header}
                                            </TableHead>
                                        ))}
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {rows.length > 0 ? (
                                        rows.map((row, rowIndex) => (
                                            <TableRow
                                                key={getRowId(row, rowIndex)}
                                                className="break-inside-avoid border-border/50 hover:bg-row-hover/50"
                                            >
                                                {columns.map((column) => (
                                                    <TableCell
                                                        key={column.id}
                                                        className={cn(
                                                            "px-2.5 py-2 align-top whitespace-normal first:ps-2 last:pe-2",
                                                            alignmentClasses[
                                                                column.align ??
                                                                    "start"
                                                            ],
                                                            column.numeric &&
                                                                "num",
                                                        )}
                                                    >
                                                        {column.cell(
                                                            row,
                                                            rowIndex,
                                                        )}
                                                    </TableCell>
                                                ))}
                                            </TableRow>
                                        ))
                                    ) : (
                                        <TableRow className="hover:bg-transparent">
                                            <TableCell
                                                colSpan={columns.length}
                                                className="py-6 text-center text-xs text-muted-foreground"
                                            >
                                                {emptyContent}
                                            </TableCell>
                                        </TableRow>
                                    )}
                                </TableBody>
                            </Table>
                        </div>
                    </section>

                    {afterLineItems}

                    {remarks != null ? (
                        <div className="mt-4 grid gap-4 md:grid-cols-[minmax(0,1fr)_minmax(19rem,23rem)] md:items-start">
                            <section
                                aria-label="备注"
                                className="min-w-0 break-inside-avoid rounded-lg border border-border/60 bg-surface-sunken/30 px-3.5 py-2.5"
                            >
                                <h2 className="text-xs font-medium text-muted-foreground">
                                    备注
                                </h2>
                                <div className="mt-1 text-xs text-foreground/85">
                                    {remarks}
                                </div>
                            </section>
                            {totalsSection}
                        </div>
                    ) : (
                        <div className="mt-4 flex justify-end">
                            {totalsSection}
                        </div>
                    )}

                    {signature != null || seal != null ? (
                        <section
                            aria-label="签章"
                            className="mt-4 grid break-inside-avoid grid-cols-2 items-end gap-6 border-t border-border/60 pt-3"
                        >
                            <div className="min-w-0">{signature}</div>
                            <div className="flex min-w-0 justify-end">
                                {seal}
                            </div>
                        </section>
                    ) : null}

                    {appendix}

                    <footer className="mt-4 break-inside-avoid border-t border-border/50 pt-2.5 text-xs text-muted-foreground">
                        {footer ?? (
                            <div className="flex flex-wrap items-center justify-between gap-4">
                                <span>{documentText.printFooter}</span>
                                <span>{documentText.effectiveVersionNote}</span>
                            </div>
                        )}
                    </footer>
                </article>
            </div>
        )
    }

    return (
        <div
            data-slot="paper-document-viewport"
            data-frame={frame}
            className={cn(
                "w-full overflow-x-auto print:overflow-visible print:border-0 print:bg-transparent print:p-0",
                frame === "framed" &&
                    "rounded-lg border border-border bg-surface-sunken p-3 sm:p-6",
                frame !== "framed" && "bg-transparent p-0",
            )}
        >
            <article
                data-slot="paper-document"
                aria-label={ariaLabel ?? "纸质单据预览"}
                className={cn(
                    "mx-auto min-w-3xl max-w-5xl text-sm text-card-foreground print:min-h-0 print:min-w-0 print:max-w-none print:border-0 print:px-0 print:py-0 print:shadow-none",
                    frame !== "framed"
                        ? "min-h-0 px-6 py-6"
                        : "min-h-screen px-10 py-12",
                    frame !== "borderless" && "border border-border bg-card",
                    frame === "framed" && "shadow-lg",
                    frame === "bare" && "shadow-2xl",
                    className,
                )}
                {...props}
            >
                <header className="break-inside-avoid">
                    {issuer != null ? (
                        <div className="mb-6 text-center text-sm font-medium tracking-wide text-muted-foreground">
                            {issuer}
                        </div>
                    ) : null}

                    <div className="grid grid-cols-3 items-end gap-6">
                        <div className="min-w-0 text-sm text-muted-foreground">
                            {subtitle}
                        </div>
                        <div className="text-center">
                            <h1 className="font-heading text-3xl font-semibold tracking-widest">
                                {title}
                            </h1>
                            <Separator className="mt-3 bg-foreground" />
                        </div>
                        <div className="flex min-w-0 justify-end">
                            {status != null ? (
                                <StatusBadge
                                    tone={status.tone}
                                    label={status.label}
                                    className="print:border-border print:bg-transparent print:text-foreground"
                                />
                            ) : null}
                        </div>
                    </div>

                    <dl className="mt-8 flex items-center justify-end gap-6 text-sm">
                        <div className="flex items-baseline gap-2">
                            <dt className="text-muted-foreground">单据编号</dt>
                            <dd className="num font-medium">
                                {documentNumber}
                            </dd>
                        </div>
                        {version != null ? (
                            <div className="flex items-baseline gap-2">
                                <dt className="text-muted-foreground">版本</dt>
                                <dd className="num font-medium">{version}</dd>
                            </div>
                        ) : null}
                    </dl>
                </header>

                <section
                    aria-label="交易双方"
                    className="mt-6 break-inside-avoid"
                >
                    <div className="grid grid-cols-2 divide-x divide-border border border-border">
                        {parties.map((party) => (
                            <div key={party.id} className="min-w-0 p-5">
                                <div className="flex items-baseline justify-between gap-4">
                                    <h2 className="text-sm font-semibold">
                                        {party.label}
                                    </h2>
                                    {party.reference != null ? (
                                        <div className="num text-xs text-muted-foreground">
                                            {party.reference}
                                        </div>
                                    ) : null}
                                </div>
                                <div className="mt-3 font-heading text-base font-semibold">
                                    {party.name}
                                </div>
                                {party.fields != null &&
                                party.fields.length > 0 ? (
                                    <dl className="mt-4 grid grid-cols-2 gap-x-6 gap-y-2 text-xs">
                                        {party.fields.map((field) => (
                                            <div
                                                key={field.id}
                                                className="min-w-0"
                                            >
                                                <dt className="text-muted-foreground">
                                                    {field.label}
                                                </dt>
                                                <dd
                                                    className={cn(
                                                        "mt-1 break-words text-foreground",
                                                        field.numeric && "num",
                                                    )}
                                                >
                                                    {field.value}
                                                </dd>
                                            </div>
                                        ))}
                                    </dl>
                                ) : null}
                            </div>
                        ))}
                    </div>
                </section>

                {metadata.length > 0 ? (
                    <section
                        aria-label="单据信息"
                        className="break-inside-avoid"
                    >
                        <dl className="grid grid-cols-4 border-x border-b border-border">
                            {metadata.map((field) => (
                                <div
                                    key={field.id}
                                    className="min-w-0 border-r border-border p-3 last:border-r-0"
                                >
                                    <dt className="text-xs text-muted-foreground">
                                        {field.label}
                                    </dt>
                                    <dd
                                        className={cn(
                                            "mt-1 text-sm font-medium",
                                            field.numeric && "num",
                                        )}
                                    >
                                        {field.value}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    </section>
                ) : null}

                <section aria-label={lineItemLabel} className="mt-8">
                    <h2 className="mb-3 text-sm font-semibold">
                        {lineItemLabel}
                    </h2>
                    <div className="overflow-hidden rounded-sm border border-grid">
                        <Table className="min-w-full">
                            <TableHeader className="print:table-header-group">
                                <TableRow className="hover:bg-transparent">
                                    {columns.map((column) => (
                                        <TableHead
                                            key={column.id}
                                            scope="col"
                                            className={cn(
                                                "border-r border-grid px-3 last:border-r-0",
                                                alignmentClasses[
                                                    column.align ?? "start"
                                                ],
                                                column.numeric && "num",
                                            )}
                                        >
                                            {column.header}
                                        </TableHead>
                                    ))}
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {rows.length > 0 ? (
                                    rows.map((row, rowIndex) => (
                                        <TableRow
                                            key={getRowId(row, rowIndex)}
                                            className="break-inside-avoid hover:bg-transparent"
                                        >
                                            {columns.map((column) => (
                                                <TableCell
                                                    key={column.id}
                                                    className={cn(
                                                        "border-r border-grid px-3 py-3 whitespace-normal last:border-r-0",
                                                        alignmentClasses[
                                                            column.align ??
                                                                "start"
                                                        ],
                                                        column.numeric && "num",
                                                    )}
                                                >
                                                    {column.cell(row, rowIndex)}
                                                </TableCell>
                                            ))}
                                        </TableRow>
                                    ))
                                ) : (
                                    <TableRow className="hover:bg-transparent">
                                        <TableCell
                                            colSpan={columns.length}
                                            className="py-8 text-center text-muted-foreground"
                                        >
                                            {emptyContent}
                                        </TableCell>
                                    </TableRow>
                                )}
                            </TableBody>
                        </Table>
                    </div>
                </section>

                {afterLineItems}
                <section
                    aria-label="金额汇总"
                    className="mt-6 flex break-inside-avoid justify-end"
                >
                    <dl className="w-full max-w-md divide-y divide-border border-y border-border">
                        {totals.map((total) => (
                            <div
                                key={total.id}
                                className="grid grid-cols-2 items-baseline gap-6 py-3"
                            >
                                <dt className="text-sm text-muted-foreground">
                                    {total.label}
                                    {total.description != null ? (
                                        <span className="mt-1 block text-xs">
                                            {total.description}
                                        </span>
                                    ) : null}
                                </dt>
                                <dd
                                    className={cn(
                                        "num text-right text-sm font-medium",
                                        total.emphasized &&
                                            "text-lg font-semibold",
                                        taxAmountToneClass(total.label),
                                    )}
                                >
                                    {total.value}
                                </dd>
                            </div>
                        ))}
                    </dl>
                </section>

                {remarks != null ? (
                    <section
                        aria-label="备注"
                        className="mt-8 break-inside-avoid border-y border-border py-4"
                    >
                        <h2 className="text-sm font-semibold">备注</h2>
                        <div className="mt-2 text-sm leading-6 text-muted-foreground">
                            {remarks}
                        </div>
                    </section>
                ) : null}

                {signature != null || seal != null ? (
                    <section
                        aria-label="签章"
                        className="mt-10 grid break-inside-avoid grid-cols-2 gap-10"
                    >
                        <div className="min-w-0">{signature}</div>
                        <div className="flex min-w-0 justify-end">{seal}</div>
                    </section>
                ) : null}

                {appendix}

                <footer className="mt-12 break-inside-avoid border-t border-border pt-4 text-xs text-muted-foreground">
                    {footer ?? (
                        <div className="flex items-center justify-between gap-6">
                            <span>{documentText.printFooter}</span>
                            <span>{documentText.effectiveVersionNote}</span>
                        </div>
                    )}
                </footer>
            </article>
        </div>
    )
}

export {
    PaperDocument,
    type PaperDocumentColumn,
    type PaperDocumentColumnAlignment,
    type PaperDocumentField,
    type PaperDocumentFrame,
    type PaperDocumentParty,
    type PaperDocumentProps,
    type PaperDocumentStatus,
    type PaperDocumentTotal,
}
