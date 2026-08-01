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

interface PaperDocumentProps<Row>
  extends Omit<React.ComponentProps<"article">, "children" | "title"> {
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
  remarks?: React.ReactNode
  signature?: React.ReactNode
  seal?: React.ReactNode
  footer?: React.ReactNode
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
  remarks,
  signature,
  seal,
  footer,
  className,
  "aria-label": ariaLabel,
  ...props
}: PaperDocumentProps<Row>) {
  return (
    <div
      data-slot="paper-document-viewport"
      className="w-full overflow-x-auto rounded-lg border border-border bg-surface-sunken p-3 sm:p-6 print:overflow-visible print:border-0 print:bg-transparent print:p-0"
    >
      <article
        data-slot="paper-document"
        aria-label={ariaLabel ?? "纸质单据预览"}
        className={cn(
          "mx-auto min-h-screen min-w-3xl max-w-5xl border border-border bg-card px-10 py-12 text-card-foreground shadow-lg print:min-h-0 print:min-w-0 print:max-w-none print:border-0 print:px-0 print:py-0 print:shadow-none",
          className
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
              <dd className="num font-medium">{documentNumber}</dd>
            </div>
            {version != null ? (
              <div className="flex items-baseline gap-2">
                <dt className="text-muted-foreground">版本</dt>
                <dd className="num font-medium">{version}</dd>
              </div>
            ) : null}
          </dl>
        </header>

        <section aria-label="交易双方" className="mt-6 break-inside-avoid">
          <div className="grid grid-cols-2 divide-x divide-border border border-border">
            {parties.map((party) => (
              <div key={party.id} className="min-w-0 p-5">
                <div className="flex items-baseline justify-between gap-4">
                  <h2 className="text-sm font-semibold">{party.label}</h2>
                  {party.reference != null ? (
                    <div className="num text-xs text-muted-foreground">
                      {party.reference}
                    </div>
                  ) : null}
                </div>
                <div className="mt-3 font-heading text-base font-semibold">
                  {party.name}
                </div>
                {party.fields != null && party.fields.length > 0 ? (
                  <dl className="mt-4 grid grid-cols-2 gap-x-6 gap-y-2 text-xs">
                    {party.fields.map((field) => (
                      <div key={field.id} className="min-w-0">
                        <dt className="text-muted-foreground">{field.label}</dt>
                        <dd
                          className={cn(
                            "mt-1 break-words text-foreground",
                            field.numeric && "num"
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
          <section aria-label="单据信息" className="break-inside-avoid">
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
                      field.numeric && "num"
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
          <h2 className="mb-3 text-sm font-semibold">{lineItemLabel}</h2>
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
                        alignmentClasses[column.align ?? "start"],
                        column.numeric && "num"
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
                            alignmentClasses[column.align ?? "start"],
                            column.numeric && "num"
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
                    total.emphasized && "text-lg font-semibold"
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
  type PaperDocumentParty,
  type PaperDocumentProps,
  type PaperDocumentStatus,
  type PaperDocumentTotal,
}
