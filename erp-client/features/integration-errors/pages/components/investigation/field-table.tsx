import type { Ref } from "react"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { cn } from "@/lib/utils"

import {
    EVIDENCE_KIND_LABEL,
    type IntegrationResolutionItemView,
} from "../../../types"

const PAGE_SIZE = 20
const FILTER_THRESHOLD = 8

export type InvestigationFieldView = { filter: string; page: number }

export function InvestigationFieldTable({
    item,
    idPrefix,
    headingRef,
    view,
    onViewChange,
    highlighted = false,
}: {
    item: IntegrationResolutionItemView
    idPrefix: string
    headingRef: Ref<HTMLHeadingElement>
    view: InvestigationFieldView
    onViewChange: (view: InvestigationFieldView) => void
    highlighted?: boolean
}) {
    const { filter, page } = view
    const difference = item.difference
    const rows = [
        ...(difference
            ? [
                  {
                      key: "external",
                      label: difference.leftLabel,
                      value: difference.leftSummary,
                  },
                  {
                      key: "internal",
                      label: difference.rightLabel,
                      value: difference.rightSummary,
                  },
              ]
            : []),
        ...(item.message
            ? [
                  {
                      key: "message",
                      label: "关联消息",
                      value: item.message.eventIdSummary,
                  },
                  {
                      key: "direction",
                      label: "消息方向",
                      value: item.message.directionLabel,
                  },
              ]
            : []),
        ...(item.originalAction
            ? [
                  {
                      key: "action",
                      label: "原操作",
                      value: item.originalAction.actionLabel,
                  },
              ]
            : []),
        ...item.linkedEvidence.map((evidence) => ({
            key: `evidence-${evidence.kind}-${evidence.recordId}`,
            label: EVIDENCE_KIND_LABEL[evidence.kind],
            value: evidence.label,
        })),
    ]
    if (!rows.length) return null

    const term = filter.trim().toLowerCase()
    const visible = rows.filter(
        ({ label, value }) =>
            !term || `${label} ${value}`.toLowerCase().includes(term),
    )
    const pageCount = Math.ceil(visible.length / PAGE_SIZE)
    const currentPage = Math.min(page, Math.max(pageCount - 1, 0))
    const offset = currentPage * PAGE_SIZE

    return (
        <section
            aria-labelledby={`${idPrefix}-fields-heading`}
            className="min-w-0 space-y-2 border-t border-border/60 pt-3.5"
        >
            <h2
                ref={headingRef}
                tabIndex={-1}
                id={`${idPrefix}-fields-heading`}
                className="scroll-mt-3 text-section-title font-semibold focus-visible:outline-ring"
            >
                {difference ? "差异明细" : "核对字段"}
            </h2>
            {rows.length > FILTER_THRESHOLD || filter ? (
                <Input
                    id={`${idPrefix}-fields-filter`}
                    aria-label="筛选核对字段"
                    placeholder="筛选项目或记录"
                    className="w-full max-w-64"
                    value={filter}
                    onChange={(event) =>
                        onViewChange({ filter: event.target.value, page: 0 })
                    }
                />
            ) : null}
            <Table className="w-full table-fixed text-left wrap-anywhere">
                <TableHeader className="bg-transparent [&_tr]:border-b-0">
                    <TableRow>
                        <TableHead
                            scope="col"
                            className="h-auto w-1/3 rounded-l-md bg-surface-sunken px-3 py-2 whitespace-normal hover:bg-surface-sunken"
                        >
                            核对项目
                        </TableHead>
                        <TableHead
                            scope="col"
                            className="h-auto rounded-r-md bg-surface-sunken px-3 py-2 whitespace-normal hover:bg-surface-sunken"
                        >
                            记录
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {visible.slice(offset, offset + PAGE_SIZE).map((row) => (
                        <TableRow
                            key={row.key}
                            className={cn(
                                "border-border/60 [&>*]:align-top",
                                highlighted &&
                                    (row.key === "external" ||
                                        row.key === "internal") &&
                                    "bg-info-soft/35 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                            )}
                        >
                            <TableHead
                                scope="row"
                                className="h-auto bg-transparent px-3 py-2 whitespace-normal hover:bg-transparent"
                            >
                                {row.label}
                            </TableHead>
                            <TableCell className="num h-auto px-3 py-2 whitespace-normal">
                                {row.value || "—"}
                            </TableCell>
                        </TableRow>
                    ))}
                    {!visible.length ? (
                        <TableRow>
                            <TableCell
                                colSpan={2}
                                className="px-3 py-3 text-muted-foreground"
                            >
                                没有匹配的记录
                            </TableCell>
                        </TableRow>
                    ) : null}
                </TableBody>
                <TableFooter className="border-t-0 bg-transparent font-normal">
                    <TableRow>
                        <TableCell
                            colSpan={2}
                            className="h-auto rounded-md bg-surface-sunken px-3 py-2 whitespace-normal"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <span
                                    aria-live="polite"
                                    className="num text-muted-foreground"
                                >
                                    共 {rows.length} 项
                                    {term ? ` · 匹配 ${visible.length} 项` : ""}
                                </span>
                                {pageCount > 1 ? (
                                    <div className="flex items-center gap-1">
                                        <Button
                                            id={`${idPrefix}-fields-prev`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label="上一页核对字段"
                                            disabled={currentPage === 0}
                                            onClick={() =>
                                                onViewChange({
                                                    filter,
                                                    page: currentPage - 1,
                                                })
                                            }
                                        >
                                            <ChevronLeftIcon aria-hidden="true" />
                                        </Button>
                                        <span className="num">
                                            {currentPage + 1} / {pageCount}
                                        </span>
                                        <Button
                                            id={`${idPrefix}-fields-next`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label="下一页核对字段"
                                            disabled={
                                                currentPage + 1 >= pageCount
                                            }
                                            onClick={() =>
                                                onViewChange({
                                                    filter,
                                                    page: currentPage + 1,
                                                })
                                            }
                                        >
                                            <ChevronRightIcon aria-hidden="true" />
                                        </Button>
                                    </div>
                                ) : null}
                            </div>
                        </TableCell>
                    </TableRow>
                </TableFooter>
            </Table>
        </section>
    )
}
