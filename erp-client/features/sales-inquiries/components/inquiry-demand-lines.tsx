"use client"

import * as React from "react"
import { PackageIcon, Trash2Icon, TruckIcon } from "lucide-react"
import { withForm } from "@/components/form"
import { TableRowActions } from "@/components/business/table-row-actions"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Sheet,
    SheetContent,
    SheetHeader,
    SheetTitle,
    SheetDescription,
    SheetFooter,
} from "@/components/ui/sheet"
import {
    Table,
    TableHeader,
    TableBody,
    TableRow,
    TableHead,
    TableCell,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { demandFormDefaults } from "../lib/demand-form"
import { deliveryProgress } from "../lib/delivery-progress"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import type { InquiryLine, SalesInquiry } from "../types"
import { InquiryDeliveryEditor } from "./inquiry-delivery-editor"

export const InquiryDemandLines = withForm({
    defaultValues: demandFormDefaults,
    props: { inquiry: undefined as SalesInquiry | undefined, disabled: false },
    render: function DemandLines({ form, inquiry, disabled }) {
        const [activeLineId, setActiveLineId] = React.useState<string | null>(
            null,
        )
        const [removed, setRemoved] = React.useState<{
            line: InquiryLine
            index: number
        } | null>(null)
        const returnFocus = React.useRef<HTMLButtonElement | null>(null)
        return (
            <form.Subscribe selector={(state) => state.values.draft.lines}>
                {(lines) => {
                    const activeIndex = lines.findIndex(
                        (line) => line.line_id === activeLineId,
                    )
                    const activeLine = lines[activeIndex]
                    return (
                        <>
                            {removed && (
                                <div
                                    className="flex items-center justify-between gap-3 rounded-md bg-muted/40 px-4 py-2 text-sm"
                                    role="status"
                                >
                                    <span>
                                        已移除 {removed.line.product_name}
                                    </span>
                                    <Button
                                        id="sales-inquiry-undo-remove"
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        disabled={disabled}
                                        onClick={() => {
                                            form.insertFieldValue(
                                                "draft.lines",
                                                Math.min(
                                                    removed.index,
                                                    lines.length,
                                                ),
                                                removed.line,
                                            )
                                            setRemoved(null)
                                        }}
                                    >
                                        撤销
                                    </Button>
                                </div>
                            )}
                            {lines.length === 0 ? (
                                <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed px-6 py-12 text-center">
                                    <PackageIcon
                                        aria-hidden="true"
                                        className="mb-1 size-7 text-muted-foreground"
                                    />
                                    <p className="text-sm font-medium">
                                        添加客户需要询价的商品
                                    </p>
                                    <p className="max-w-md text-sm leading-6 text-muted-foreground">
                                        从公司商品池选择商品，填写数量与交期。创建草稿后，也可导入客户选品文件。
                                    </p>
                                </div>
                            ) : (
                                <div className="min-w-0 overflow-hidden rounded-lg border [&>[data-slot=table-container]]:max-h-[65vh]">
                                    <Table className="min-w-[1280px] table-fixed">
                                        <TableHeader className="sticky top-0 z-10">
                                            <TableRow>
                                                <TableHead>
                                                    商品 / 规格
                                                </TableHead>
                                                <TableHead className="w-36">
                                                    需求数量{" "}
                                                    <span className="text-destructive">
                                                        *
                                                    </span>
                                                </TableHead>
                                                <TableHead className="w-40 text-right">
                                                    参考单价（选填）
                                                </TableHead>
                                                <TableHead className="w-52">
                                                    要求交付日
                                                </TableHead>
                                                <TableHead className="w-48">
                                                    配送 / 交付
                                                </TableHead>
                                                <TableHead className="sticky right-0 z-20 w-52 border-l text-right">
                                                    操作
                                                </TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {lines.map((line, index) => {
                                                const prefix = `sales-inquiry-line-${toAutomationIdSegment(line.line_id)}`
                                                const progress =
                                                    deliveryProgress(line)
                                                return (
                                                    <TableRow
                                                        key={line.line_id}
                                                    >
                                                        <TableCell className="whitespace-normal py-4">
                                                            <p className="font-medium leading-5">
                                                                {
                                                                    line.product_name
                                                                }
                                                            </p>
                                                            <p className="mt-1 text-xs leading-5 text-muted-foreground">
                                                                {line.specification &&
                                                                line.specification !==
                                                                    "无规格"
                                                                    ? `${line.specification} · `
                                                                    : ""}
                                                                SKU{" "}
                                                                {line.sku_code}
                                                            </p>
                                                        </TableCell>
                                                        <TableCell>
                                                            <form.AppField
                                                                name={`draft.lines[${index}].quantity`}
                                                            >
                                                                {(field) => (
                                                                    <field.TextField
                                                                        id={`${prefix}-quantity`}
                                                                        label={`${line.product_name}需求数量`}
                                                                        hideLabel
                                                                        placeholder="请输入数量"
                                                                        inputMode="decimal"
                                                                        className="w-24 shrink-0"
                                                                        inputClassName="text-right tabular-nums"
                                                                        required
                                                                    />
                                                                )}
                                                            </form.AppField>
                                                        </TableCell>
                                                        <TableCell>
                                                            <form.Field
                                                                name={`draft.lines[${index}].sales_reference_price_gross`}
                                                            >
                                                                {(field) => (
                                                                    <Input
                                                                        id={`${prefix}-reference-price`}
                                                                        aria-label={`${line.product_name}销售参考单价（元）`}
                                                                        className="text-right tabular-nums"
                                                                        inputMode="decimal"
                                                                        placeholder="¥ 选填"
                                                                        value={
                                                                            field
                                                                                .state
                                                                                .value ??
                                                                            ""
                                                                        }
                                                                        onBlur={
                                                                            field.handleBlur
                                                                        }
                                                                        onChange={(
                                                                            event,
                                                                        ) =>
                                                                            field.handleChange(
                                                                                event
                                                                                    .target
                                                                                    .value ||
                                                                                    null,
                                                                            )
                                                                        }
                                                                    />
                                                                )}
                                                            </form.Field>
                                                        </TableCell>
                                                        <TableCell>
                                                            <form.Field
                                                                name={`draft.lines[${index}].delivery_due_at`}
                                                            >
                                                                {(field) => (
                                                                    <DatePicker
                                                                        id={`${prefix}-due-date`}
                                                                        size="default"
                                                                        placeholder="选择交付日期"
                                                                        value={inquiryDateInput(
                                                                            field
                                                                                .state
                                                                                .value,
                                                                        )}
                                                                        onValueChange={(
                                                                            value,
                                                                        ) =>
                                                                            field.handleChange(
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
                                                        <TableCell>
                                                            <div className="space-y-1.5">
                                                                <Badge
                                                                    variant={
                                                                        progress.complete
                                                                            ? "secondary"
                                                                            : "warning"
                                                                    }
                                                                >
                                                                    {
                                                                        progress.label
                                                                    }
                                                                </Badge>
                                                                {line.delivery_terms && (
                                                                    <p
                                                                        className="max-w-48 truncate text-xs text-muted-foreground"
                                                                        title={
                                                                            line.delivery_terms
                                                                        }
                                                                    >
                                                                        {
                                                                            line.delivery_terms
                                                                        }
                                                                    </p>
                                                                )}
                                                            </div>
                                                        </TableCell>
                                                        <TableCell className="sticky right-0 z-1 border-l bg-card group-hover/row:bg-row-hover">
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
                                                                        id: `${prefix}-remove`,
                                                                        label: "移除商品",
                                                                        icon: Trash2Icon,
                                                                        placement:
                                                                            "inline",
                                                                        destructive: true,
                                                                        disabled,
                                                                        onClick:
                                                                            () => {
                                                                                setRemoved(
                                                                                    {
                                                                                        line,
                                                                                        index,
                                                                                    },
                                                                                )
                                                                                form.removeFieldValue(
                                                                                    "draft.lines",
                                                                                    index,
                                                                                )
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
                                            设置交付条件与配送地址，填写内容会保留在当前表单中。
                                        </SheetDescription>
                                    </SheetHeader>
                                    <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
                                        {activeLine && (
                                            <InquiryDeliveryEditor
                                                form={form}
                                                key={activeLine.line_id}
                                                line={activeLine}
                                                index={activeIndex}
                                                inquiry={inquiry}
                                                disabled={disabled}
                                            />
                                        )}
                                    </div>
                                    <SheetFooter>
                                        <p className="mr-auto text-xs text-muted-foreground">
                                            关闭后请在页面底部保存需求。
                                        </p>
                                        <Button
                                            id="sales-inquiry-delivery-done"
                                            type="button"
                                            onClick={() =>
                                                setActiveLineId(null)
                                            }
                                        >
                                            完成编辑
                                        </Button>
                                    </SheetFooter>
                                </SheetContent>
                            </Sheet>
                        </>
                    )
                }}
            </form.Subscribe>
        )
    },
})
