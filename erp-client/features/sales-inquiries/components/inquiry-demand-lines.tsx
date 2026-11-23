"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import {
    ChevronRightIcon,
    CopyIcon,
    PackageIcon,
    PlusIcon,
    Trash2Icon,
    TruckIcon,
    UploadIcon,
} from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { TableRowActions } from "@/components/business/table-row-actions"
import { TableToolbarScope } from "@/components/business/table-toolbar"
import { withForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import { SkuThumbnail } from "@/features/sales-orders/components/sku-thumbnail"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { deliveryProgress } from "../lib/delivery-progress"
import { demandFormDefaults } from "../lib/demand-form"
import {
    isLineQuantityValid,
    isLineReferencePriceValid,
    summarizeDemand,
} from "../lib/demand-summary"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import type { InquiryLine, SalesInquiry } from "../types"
import { InquiryDeliveryEditor } from "./inquiry-delivery-editor"

const INLINE_DATE_PICKER_CLASS =
    "w-full [&_[data-variant=control]]:border-transparent [&_[data-variant=control]]:bg-muted/52 [&_[data-variant=control]]:hover:bg-muted/85 [&_[data-variant=control]]:focus-visible:border-primary/65 [&_[data-variant=control]]:focus-visible:bg-card"

const FULFILLMENT_KIND_TAG: Record<InquiryLine["fulfillment_kind"], string> = {
    physical: "实物",
    virtual: "虚拟",
    service: "服务",
}

type RemovedEntry = {
    line: InquiryLine
    index: number
}

export const InquiryDemandLines = withForm({
    defaultValues: demandFormDefaults,
    props: {
        inquiry: undefined as SalesInquiry | undefined,
        disabled: false,
        selectedIds: new Set<string>() as ReadonlySet<string>,
        onSelectedIdsChange: (() => undefined) as (next: Set<string>) => void,
        onAddProducts: (() => undefined) as () => void,
        onImport: undefined as (() => void) | undefined,
    },
    render: function DemandLines({
        form,
        inquiry,
        disabled,
        selectedIds,
        onSelectedIdsChange,
        onAddProducts,
        onImport,
    }) {
        const values = useStore(form.store, (state) => state.values)
        const demandLines = values.draft.lines
        const skuDetails = useInquirySkuDetailsQuery(
            demandLines.map((line) => line.sku_id),
        )
        const unitLabel = (code: string) =>
            skuDetails.units.get(code) ?? (skuDetails.isPending ? "加载中" : "")
        const [activeLineId, setActiveLineId] = React.useState<string | null>(
            null,
        )
        const [removed, setRemoved] = React.useState<RemovedEntry[]>([])
        const returnFocus = React.useRef<HTMLButtonElement | null>(null)
        const summary = React.useMemo(() => summarizeDemand(values), [values])

        const duplicateLine = (line: InquiryLine, index: number) => {
            if (disabled) return
            const current = form.getFieldValue("draft.lines")
            if (current.length >= 200) {
                toast.add({
                    title: "已达商品数量上限",
                    description: "单张询价最多包含 200 条商品需求。",
                    type: "warning",
                })
                return
            }
            const cloned: InquiryLine = {
                ...line,
                line_id: crypto.randomUUID(),
                allocations: line.allocations.map((address) => ({
                    ...address,
                    allocation_id: crypto.randomUUID(),
                })),
            }
            form.insertFieldValue("draft.lines", index + 1, cloned)
            toast.add({
                title: "已复制商品需求行",
                description: `已复制「${line.product_name}」，可分别调整交期或配送地址。`,
                type: "success",
            })
        }

        const removeSelected = (lines: readonly InquiryLine[]) => {
            if (disabled) return
            const entries = lines.flatMap((line, index) =>
                selectedIds.has(line.line_id) ? [{ line, index }] : [],
            )
            if (entries.length === 0) return
            const ids = new Set(entries.map((entry) => entry.line.line_id))
            form.setFieldValue(
                "draft.lines",
                lines.filter((line) => !ids.has(line.line_id)),
            )
            setRemoved(entries)
            onSelectedIdsChange(new Set())
        }

        return (
            <form.Field name="draft.lines" mode="array">
                {(field) => {
                    const lines = field.state.value
                    const activeIndex = lines.findIndex(
                        (line) => line.line_id === activeLineId,
                    )
                    const activeLine = lines[activeIndex]
                    const selectedCount = lines.filter((line) =>
                        selectedIds.has(line.line_id),
                    ).length

                    return (
                        <TableToolbarScope>
                            {skuDetails.error && (
                                <BusinessFailureState
                                    id="sales-inquiry-demand-sku-details"
                                    title="商品图片与单位资料读取失败"
                                    error={skuDetails.error}
                                    onRetry={() => void skuDetails.refetch()}
                                />
                            )}

                            {lines.length > 0 && selectedCount > 0 && (
                                <div
                                    role="region"
                                    aria-label="已选商品批量操作"
                                    className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-primary/20 bg-sidebar-accent/55 px-3 py-1.5 text-body-compact"
                                >
                                    <div className="flex flex-wrap items-center gap-2">
                                        <span className="font-medium text-sidebar-accent-foreground">
                                            已选{" "}
                                            <span className="num">
                                                {selectedCount}
                                            </span>{" "}
                                            /{" "}
                                            <span className="num">
                                                {lines.length}
                                            </span>{" "}
                                            项
                                        </span>
                                        {selectedCount < lines.length && (
                                            <Button
                                                id="sales-inquiry-batch-due-date-select-all"
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                disabled={disabled}
                                                onClick={() =>
                                                    onSelectedIdsChange(
                                                        new Set(
                                                            lines.map(
                                                                (line) =>
                                                                    line.line_id,
                                                            ),
                                                        ),
                                                    )
                                                }
                                            >
                                                全选 {lines.length} 项
                                            </Button>
                                        )}
                                        <Button
                                            id="sales-inquiry-batch-remove-selected"
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={
                                                detailWorkspaceStyles.quietDangerButton
                                            }
                                            disabled={disabled}
                                            onClick={() =>
                                                removeSelected(lines)
                                            }
                                        >
                                            <Trash2Icon
                                                aria-hidden="true"
                                                data-icon="inline-start"
                                            />
                                            移除所选
                                        </Button>
                                    </div>
                                    <Button
                                        id="sales-inquiry-batch-due-date-clear-selection"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={disabled}
                                        onClick={() =>
                                            onSelectedIdsChange(new Set())
                                        }
                                    >
                                        清空选择
                                    </Button>
                                </div>
                            )}

                            {removed.length > 0 && (
                                <div
                                    className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-muted/50 px-3.5 py-2 text-body-compact"
                                    role="status"
                                >
                                    <span>
                                        {removed.length === 1
                                            ? `已移除「${removed[0]!.line.product_name}」`
                                            : `已移除 ${removed.length} 项商品`}
                                    </span>
                                    <Button
                                        id="sales-inquiry-undo-remove"
                                        type="button"
                                        size="xs"
                                        variant="ghost"
                                        className={
                                            detailWorkspaceStyles.inlineLinkButton
                                        }
                                        disabled={disabled}
                                        onClick={() => {
                                            const current = [
                                                ...form.getFieldValue(
                                                    "draft.lines",
                                                ),
                                            ]
                                            for (const entry of removed) {
                                                current.splice(
                                                    Math.min(
                                                        entry.index,
                                                        current.length,
                                                    ),
                                                    0,
                                                    entry.line,
                                                )
                                            }
                                            form.setFieldValue(
                                                "draft.lines",
                                                current,
                                            )
                                            setRemoved([])
                                        }}
                                    >
                                        撤销
                                    </Button>
                                </div>
                            )}

                            {lines.length === 0 ? (
                                <div className="flex flex-col items-center gap-3 rounded-lg border border-dashed border-border/80 bg-surface-sunken/50 px-6 py-12 text-center">
                                    <div className="flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
                                        <PackageIcon
                                            aria-hidden="true"
                                            className="size-5"
                                        />
                                    </div>
                                    <div className="space-y-1">
                                        <p className="text-body-compact font-medium text-foreground">
                                            尚未添加询价商品
                                        </p>
                                        <p className="max-w-md text-xs text-muted-foreground">
                                            {onImport
                                                ? "从公司商品池选择商品，或直接导入客户选品工作簿。"
                                                : "从公司商品池选择商品，填写数量、交付日与配送地址。"}
                                        </p>
                                    </div>
                                    <div className="mt-1 flex flex-wrap items-center justify-center gap-2">
                                        <Button
                                            id="sales-inquiry-empty-add-products"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={disabled}
                                            onClick={onAddProducts}
                                        >
                                            <PlusIcon
                                                aria-hidden="true"
                                                data-icon="inline-start"
                                            />
                                            添加商品
                                        </Button>
                                        {onImport && (
                                            <Button
                                                id="sales-inquiry-empty-import"
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                className={
                                                    detailWorkspaceStyles.subtleButton
                                                }
                                                disabled={disabled}
                                                onClick={onImport}
                                            >
                                                <UploadIcon
                                                    aria-hidden="true"
                                                    data-icon="inline-start"
                                                />
                                                导入客户选品
                                            </Button>
                                        )}
                                    </div>
                                </div>
                            ) : (
                                <div
                                    data-slot="detail-inline-table"
                                    className="min-w-0 border-t border-border/70"
                                >
                                    <Table className="min-w-[1000px] table-fixed">
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead className="w-10">
                                                    <NativeCheckbox
                                                        id="sales-inquiry-lines-select-all"
                                                        aria-label="全选商品"
                                                        checked={
                                                            selectedCount ===
                                                            lines.length
                                                        }
                                                        indeterminate={
                                                            selectedCount > 0 &&
                                                            selectedCount <
                                                                lines.length
                                                        }
                                                        disabled={disabled}
                                                        onCheckedChange={(
                                                            checked,
                                                        ) =>
                                                            onSelectedIdsChange(
                                                                checked
                                                                    ? new Set(
                                                                          lines.map(
                                                                              (
                                                                                  line,
                                                                              ) =>
                                                                                  line.line_id,
                                                                          ),
                                                                      )
                                                                    : new Set(),
                                                            )
                                                        }
                                                    />
                                                </TableHead>
                                                <TableHead>
                                                    商品 / 规格
                                                </TableHead>
                                                <TableHead
                                                    data-align="end"
                                                    className="w-32"
                                                >
                                                    需求数量
                                                    <RequiredMark />
                                                </TableHead>
                                                <TableHead
                                                    data-align="end"
                                                    className="w-36"
                                                >
                                                    销售价（选填）
                                                </TableHead>
                                                <TableHead className="w-48">
                                                    要求交付日
                                                </TableHead>
                                                <TableHead className="w-44">
                                                    配送 / 交付
                                                </TableHead>
                                                <TableHead className="sticky right-0 z-20 w-36 text-right">
                                                    操作
                                                </TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {lines.map((line, index) => {
                                                const prefix = `sales-inquiry-line-${toAutomationIdSegment(line.line_id)}`
                                                const unit = unitLabel(
                                                    line.unit_code,
                                                )
                                                const progress =
                                                    deliveryProgress({
                                                        ...line,
                                                        unit_code: unit,
                                                    })
                                                const quantityValid =
                                                    isLineQuantityValid(line)
                                                const priceValid =
                                                    isLineReferencePriceValid(
                                                        line,
                                                    )
                                                const specLine = [
                                                    line.specification &&
                                                    line.specification !==
                                                        "无规格"
                                                        ? line.specification
                                                        : null,
                                                    `SKU ${line.sku_code}`,
                                                ]
                                                    .filter(Boolean)
                                                    .join(" · ")
                                                const deliveryOk =
                                                    progress.complete &&
                                                    !progress.overAllocated

                                                return (
                                                    <TableRow
                                                        key={line.line_id}
                                                        data-state={
                                                            selectedIds.has(
                                                                line.line_id,
                                                            )
                                                                ? "selected"
                                                                : undefined
                                                        }
                                                    >
                                                        <TableCell className="py-3 align-top">
                                                            <div className="pt-1.5">
                                                                <NativeCheckbox
                                                                    id={`${prefix}-select`}
                                                                    aria-label={`选择${line.product_name}`}
                                                                    checked={selectedIds.has(
                                                                        line.line_id,
                                                                    )}
                                                                    disabled={
                                                                        disabled
                                                                    }
                                                                    onCheckedChange={(
                                                                        checked,
                                                                    ) => {
                                                                        const next =
                                                                            new Set(
                                                                                selectedIds,
                                                                            )
                                                                        if (
                                                                            checked
                                                                        )
                                                                            next.add(
                                                                                line.line_id,
                                                                            )
                                                                        else
                                                                            next.delete(
                                                                                line.line_id,
                                                                            )
                                                                        onSelectedIdsChange(
                                                                            next,
                                                                        )
                                                                    }}
                                                                />
                                                            </div>
                                                        </TableCell>
                                                        <TableCell className="whitespace-normal py-3 align-top">
                                                            <div className="flex items-start gap-2.5">
                                                                <SkuThumbnail
                                                                    className="size-10 rounded-md"
                                                                    assetId={skuDetails.images.get(
                                                                        line.sku_id,
                                                                    )}
                                                                    label={
                                                                        line.product_name
                                                                    }
                                                                />
                                                                <div className="min-w-0 flex-1">
                                                                    <div className="flex items-center gap-1.5">
                                                                        <p className="min-w-0 font-medium leading-5 text-foreground wrap-anywhere">
                                                                            {
                                                                                line.product_name
                                                                            }
                                                                        </p>
                                                                        {line.fulfillment_kind !==
                                                                            "physical" && (
                                                                            <span className="shrink-0 rounded-sm bg-muted px-1.5 py-0.5 text-tiny font-medium text-muted-foreground">
                                                                                {
                                                                                    FULFILLMENT_KIND_TAG[
                                                                                        line
                                                                                            .fulfillment_kind
                                                                                    ]
                                                                                }
                                                                            </span>
                                                                        )}
                                                                    </div>
                                                                    <p
                                                                        className="num mt-0.5 truncate text-xs text-muted-foreground"
                                                                        title={
                                                                            specLine
                                                                        }
                                                                    >
                                                                        {
                                                                            specLine
                                                                        }
                                                                    </p>
                                                                </div>
                                                            </div>
                                                        </TableCell>
                                                        <TableCell
                                                            data-align="end"
                                                            className="py-3 align-top"
                                                        >
                                                            <form.Field
                                                                name={`draft.lines[${index}].quantity`}
                                                            >
                                                                {(qtyField) => (
                                                                    <div className="flex items-center justify-end gap-1.5">
                                                                        <Input
                                                                            id={`${prefix}-quantity`}
                                                                            aria-label={`${line.product_name}需求数量`}
                                                                            aria-invalid={
                                                                                !quantityValid ||
                                                                                undefined
                                                                            }
                                                                            title={
                                                                                quantityValid
                                                                                    ? undefined
                                                                                    : "请输入大于 0 的有效数量"
                                                                            }
                                                                            inputMode="decimal"
                                                                            placeholder="数量"
                                                                            aria-required="true"
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            className="h-control-sm num min-w-0 flex-1 text-right"
                                                                            value={
                                                                                qtyField
                                                                                    .state
                                                                                    .value
                                                                            }
                                                                            onBlur={
                                                                                qtyField.handleBlur
                                                                            }
                                                                            onChange={(
                                                                                event,
                                                                            ) =>
                                                                                qtyField.handleChange(
                                                                                    event
                                                                                        .target
                                                                                        .value,
                                                                                )
                                                                            }
                                                                        />
                                                                        {unit ? (
                                                                            <span className="shrink-0 text-xs text-muted-foreground">
                                                                                {
                                                                                    unit
                                                                                }
                                                                            </span>
                                                                        ) : null}
                                                                    </div>
                                                                )}
                                                            </form.Field>
                                                        </TableCell>
                                                        <TableCell
                                                            data-align="end"
                                                            className="py-3 align-top"
                                                        >
                                                            <form.Field
                                                                name={`draft.lines[${index}].sales_reference_price_gross`}
                                                            >
                                                                {(
                                                                    priceField,
                                                                ) => (
                                                                    <div className="relative min-w-0">
                                                                        <span
                                                                            aria-hidden="true"
                                                                            className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-xs text-muted-foreground/70"
                                                                        >
                                                                            ¥
                                                                        </span>
                                                                        <Input
                                                                            id={`${prefix}-reference-price`}
                                                                            aria-label={`${line.product_name}销售价（元）`}
                                                                            aria-invalid={
                                                                                !priceValid ||
                                                                                undefined
                                                                            }
                                                                            title={
                                                                                priceValid
                                                                                    ? undefined
                                                                                    : "请输入非负金额（最多 4 位小数）"
                                                                            }
                                                                            className="h-control-sm num pl-6 text-right"
                                                                            inputMode="decimal"
                                                                            placeholder="选填"
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            value={
                                                                                priceField
                                                                                    .state
                                                                                    .value ??
                                                                                ""
                                                                            }
                                                                            onBlur={
                                                                                priceField.handleBlur
                                                                            }
                                                                            onChange={(
                                                                                event,
                                                                            ) =>
                                                                                priceField.handleChange(
                                                                                    event.target.value.replaceAll(
                                                                                        "¥",
                                                                                        "",
                                                                                    ) ||
                                                                                        null,
                                                                                )
                                                                            }
                                                                        />
                                                                    </div>
                                                                )}
                                                            </form.Field>
                                                        </TableCell>
                                                        <TableCell className="py-3 align-top">
                                                            <form.Field
                                                                name={`draft.lines[${index}].delivery_due_at`}
                                                            >
                                                                {(
                                                                    dateField,
                                                                ) => (
                                                                    <DatePicker
                                                                        id={`${prefix}-due-date`}
                                                                        size="sm"
                                                                        placeholder="选择日期"
                                                                        disabled={
                                                                            disabled
                                                                        }
                                                                        className={
                                                                            INLINE_DATE_PICKER_CLASS
                                                                        }
                                                                        value={
                                                                            inquiryDateInput(
                                                                                dateField
                                                                                    .state
                                                                                    .value,
                                                                            ) ||
                                                                            undefined
                                                                        }
                                                                        onValueChange={(
                                                                            value,
                                                                        ) =>
                                                                            dateField.handleChange(
                                                                                inquiryDateSeconds(
                                                                                    value ??
                                                                                        "",
                                                                                ),
                                                                            )
                                                                        }
                                                                    />
                                                                )}
                                                            </form.Field>
                                                        </TableCell>
                                                        <TableCell className="whitespace-normal py-3 align-top">
                                                            <button
                                                                id={`${prefix}-delivery-trigger`}
                                                                type="button"
                                                                disabled={
                                                                    disabled
                                                                }
                                                                onClick={(
                                                                    event,
                                                                ) => {
                                                                    returnFocus.current =
                                                                        event.currentTarget
                                                                    setActiveLineId(
                                                                        line.line_id,
                                                                    )
                                                                }}
                                                                className={cn(
                                                                    "group/delivery -mx-1.5 -my-1 flex w-[calc(100%+0.75rem)] flex-col items-start gap-1 rounded-md px-1.5 py-1 text-left transition-colors",
                                                                    "hover:bg-muted/70 focus-visible:outline-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-60",
                                                                )}
                                                            >
                                                                <span className="flex w-full items-center justify-between gap-1">
                                                                    <StatusBadge
                                                                        className="h-auto min-h-5 max-w-full justify-start text-left leading-5 whitespace-normal wrap-anywhere"
                                                                        tone={
                                                                            deliveryOk
                                                                                ? "neutral"
                                                                                : "warning"
                                                                        }
                                                                        label={
                                                                            progress.label
                                                                        }
                                                                    />
                                                                    <ChevronRightIcon
                                                                        aria-hidden="true"
                                                                        className="size-3.5 shrink-0 text-muted-foreground/45 transition-colors group-hover/delivery:text-primary"
                                                                    />
                                                                </span>
                                                                {line.delivery_terms && (
                                                                    <span
                                                                        className="line-clamp-2 text-xs text-muted-foreground wrap-anywhere"
                                                                        title={
                                                                            line.delivery_terms
                                                                        }
                                                                    >
                                                                        {
                                                                            line.delivery_terms
                                                                        }
                                                                    </span>
                                                                )}
                                                            </button>
                                                        </TableCell>
                                                        <TableCell className="sticky right-0 z-1 bg-card py-3 align-top group-hover/row:bg-row-hover group-data-[state=selected]/row:bg-row-selected">
                                                            <TableRowActions
                                                                moreId={`${prefix}-more`}
                                                                moreLabel={`${line.product_name}更多操作`}
                                                                moreIconOnly
                                                                actions={[
                                                                    {
                                                                        id: `${prefix}-edit-delivery`,
                                                                        label:
                                                                            line.fulfillment_kind ===
                                                                            "physical"
                                                                                ? "配送"
                                                                                : "交付",
                                                                        icon: TruckIcon,
                                                                        disabled,
                                                                        onClick:
                                                                            (
                                                                                event,
                                                                            ) => {
                                                                                returnFocus.current =
                                                                                    event.currentTarget as HTMLButtonElement
                                                                                setActiveLineId(
                                                                                    line.line_id,
                                                                                )
                                                                            },
                                                                    },
                                                                    {
                                                                        id: `${prefix}-duplicate`,
                                                                        label: "复制一行",
                                                                        icon: CopyIcon,
                                                                        placement:
                                                                            "menu",
                                                                        disabled,
                                                                        onClick:
                                                                            () =>
                                                                                duplicateLine(
                                                                                    line,
                                                                                    index,
                                                                                ),
                                                                    },
                                                                    {
                                                                        id: `${prefix}-remove`,
                                                                        label: "移除",
                                                                        icon: Trash2Icon,
                                                                        placement:
                                                                            "inline",
                                                                        destructive: true,
                                                                        disabled,
                                                                        onClick:
                                                                            () => {
                                                                                setRemoved(
                                                                                    [
                                                                                        {
                                                                                            line,
                                                                                            index,
                                                                                        },
                                                                                    ],
                                                                                )
                                                                                form.removeFieldValue(
                                                                                    "draft.lines",
                                                                                    index,
                                                                                )
                                                                                if (
                                                                                    selectedIds.has(
                                                                                        line.line_id,
                                                                                    )
                                                                                ) {
                                                                                    const next =
                                                                                        new Set(
                                                                                            selectedIds,
                                                                                        )
                                                                                    next.delete(
                                                                                        line.line_id,
                                                                                    )
                                                                                    onSelectedIdsChange(
                                                                                        next,
                                                                                    )
                                                                                }
                                                                            },
                                                                    },
                                                                ]}
                                                            />
                                                        </TableCell>
                                                    </TableRow>
                                                )
                                            })}
                                        </TableBody>
                                    </Table>
                                    <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border/70 py-2">
                                        <Button
                                            id="sales-inquiry-lines-inline-add"
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            className={
                                                detailWorkspaceStyles.inlineLinkButton
                                            }
                                            disabled={disabled}
                                            onClick={onAddProducts}
                                        >
                                            <PlusIcon
                                                aria-hidden="true"
                                                data-icon="inline-start"
                                            />
                                            继续添加商品
                                        </Button>
                                        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
                                            <span>
                                                共{" "}
                                                <span className="num font-medium text-foreground">
                                                    {summary.lineCount}
                                                </span>{" "}
                                                项商品
                                            </span>
                                            {summary.referenceTotal !== null ? (
                                                <span>
                                                    销售参考合计{" "}
                                                    <MoneyValue
                                                        value={
                                                            summary.referenceTotal
                                                        }
                                                        display="exact"
                                                        showHint={false}
                                                        className="font-medium text-foreground"
                                                    />
                                                </span>
                                            ) : null}
                                        </div>
                                    </div>
                                </div>
                            )}

                            <Sheet
                                open={Boolean(activeLine)}
                                onOpenChange={(open) => {
                                    if (!open) setActiveLineId(null)
                                }}
                            >
                                <SheetContent
                                    size="detail"
                                    closeButtonId="sales-inquiry-delivery-close"
                                    finalFocus={returnFocus}
                                >
                                    <SheetHeader>
                                        <SheetTitle>
                                            {activeLine?.product_name ??
                                                "配送与交付"}
                                        </SheetTitle>
                                        <SheetDescription>
                                            配置配送数量、收件地址与交付条件
                                            {inquiry
                                                ? "；已有销售占用的地址不可替换或移除。"
                                                : "。"}
                                        </SheetDescription>
                                    </SheetHeader>
                                    <div className="min-h-0 flex-1 overflow-y-auto px-sheet-inline pb-5">
                                        {activeLine && (
                                            <InquiryDeliveryEditor
                                                form={form}
                                                key={activeLine.line_id}
                                                line={activeLine}
                                                unit={unitLabel(
                                                    activeLine.unit_code,
                                                )}
                                                index={activeIndex}
                                                inquiry={inquiry}
                                                disabled={disabled}
                                            />
                                        )}
                                    </div>
                                    <SheetFooter>
                                        <p className="mr-auto text-xs text-muted-foreground">
                                            {inquiry
                                                ? "关闭后请在页面保存需求。"
                                                : "关闭后点击右上角“创建询价草稿”保存。"}
                                        </p>
                                        <Button
                                            id="sales-inquiry-delivery-done"
                                            type="button"
                                            onClick={() =>
                                                setActiveLineId(null)
                                            }
                                        >
                                            完成
                                        </Button>
                                    </SheetFooter>
                                </SheetContent>
                            </Sheet>
                        </TableToolbarScope>
                    )
                }}
            </form.Field>
        )
    },
})
