"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useAppForm } from "@/components/form"
import { QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { useRegionIndex } from "@/features/regions"
import { getErrorMessage } from "@/lib/api/errors"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import {
    addressImportDefaults,
    parseAddressText,
} from "../lib/address-text-import"
import {
    buildAddressImportPlan,
    type AddressImportChanges,
    type AddressImportChoice,
} from "../lib/address-import-plan"
import { deliveryProgress } from "../lib/delivery-progress"
import type { InquiryLine, SalesInquiry } from "../types"
import { InquiryAddressImportGroups } from "./inquiry-address-import-groups"

export function InquiryAddressImportDialog({
    prefix,
    open,
    onOpenChange,
    line,
    inquiry,
    unit,
    disabled,
    onApply,
}: {
    prefix: string
    open: boolean
    onOpenChange: (open: boolean) => void
    line: InquiryLine
    inquiry?: SalesInquiry
    unit?: string
    disabled: boolean
    onApply: (changes: AddressImportChanges) => void
}) {
    const regions = useRegionIndex()
    const [failure, setFailure] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: {
            ...addressImportDefaults,
            choices: {} as Record<string, AddressImportChoice>,
        },
        onSubmit: ({ value }) => {
            if (disabled) return
            setFailure(null)
            try {
                if (!value.rows.length) {
                    form.setFieldValue("rows", parseAddressText(value.text))
                    form.setFieldValue("choices", {})
                    return
                }
                if (!regions.data)
                    throw new Error("地区资料尚未加载，请稍后重试。")
                const plan = buildAddressImportPlan(
                    value.rows,
                    line,
                    regions.data,
                    value.choices,
                    inquiry,
                )
                if (plan.hasErrors)
                    throw new Error("请修正错误或移除错误组后再确认。")
                if (plan.conflicts)
                    throw new Error("请选择各组数量冲突的处理方式。")
                if (plan.overLimit)
                    throw new Error("处理后超过 100 个地址，请移除多余组。")
                if (!plan.changed) throw new Error("全部已跳过，无需修改地址。")
                onApply(plan.changes)
                form.reset()
                onOpenChange(false)
            } catch (error) {
                setFailure(getErrorMessage(error))
            }
        },
    })
    const { rows, choices } = useStore(form.store, (state) => state.values)
    const plan = regions.data
        ? buildAddressImportPlan(rows, line, regions.data, choices, inquiry)
        : null
    const ready = Boolean(plan && !plan.hasErrors && !plan.conflicts)
    const allSkipped = rows.length > 0 && ready && plan?.changed === 0
    let total: string | null = null
    let exceedsDemand = false
    if (ready && plan) {
        try {
            total = sumFixed(
                plan.allocations.map((row) => row.quantity),
                {
                    maxScale: line.quantity_scale,
                    outputScale: line.quantity_scale,
                },
            )
            exceedsDemand =
                compareDecimal(total, line.quantity, line.quantity_scale) > 0
        } catch {
            /* 既有草稿数量未完善时，沿用配送进度提示。 */
        }
    }
    const progress =
        ready && plan
            ? deliveryProgress({
                  ...line,
                  unit_code: unit ?? "",
                  allocations: plan.allocations,
              })
            : null
    const id = `${prefix}-address-import`
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId={`${id}-close`}
                className="flex max-h-[90vh] flex-col overflow-hidden sm:max-w-6xl"
                finalFocus={() =>
                    document.getElementById(`${prefix}-import-addresses`)
                }
            >
                <DialogHeader>
                    <DialogTitle>批量添加配送地址</DialogTitle>
                    <DialogDescription>
                        {line.product_name} ·{" "}
                        {rows.length
                            ? "核对去重结果与数量变更，确认后统一应用。"
                            : "直接粘贴 Excel 行，支持空格或 Tab 分隔。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex min-h-0 flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        void form.handleSubmit()
                    }}
                >
                    <div className="min-h-0 space-y-3 overflow-y-auto">
                        {failure && (
                            <Alert variant="destructive">
                                <AlertDescription>{failure}</AlertDescription>
                            </Alert>
                        )}
                        {!rows.length ? (
                            <>
                                <p className="text-sm">
                                    列顺序：收件人、联系电话、省、市、区、详细地址、配送数量（选填，默认
                                    1）。
                                </p>
                                <form.AppField name="text">
                                    {(field) => (
                                        <field.TextareaField
                                            id={`${id}-text`}
                                            label="粘贴收件信息"
                                            rows={9}
                                            disabled={disabled}
                                            placeholder={
                                                "张三\t13800138000\t广东省\t广州市\t天河区\t体育西路 100 号\t2\n李四 13900139000 广东省 深圳市 南山区 科技园1号"
                                            }
                                            description="每行一个地址，可包含表头。空格分隔时，含空格的姓名或详细地址请用英文双引号包裹；从 Excel 复制时可直接粘贴。"
                                        />
                                    )}
                                </form.AppField>
                                <p className="text-xs text-muted-foreground">
                                    现有 {line.allocations.length}{" "}
                                    个地址；按去重后的结果校验最多 100
                                    个地址。相同资料和数量默认跳过，不自动累加。
                                </p>
                            </>
                        ) : (
                            <>
                                {regions.isError && (
                                    <Alert variant="destructive">
                                        <AlertDescription>
                                            地区资料加载失败。
                                            <Button
                                                id={`${id}-retry-regions`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                onClick={() =>
                                                    void regions.refetch()
                                                }
                                            >
                                                重试
                                            </Button>
                                        </AlertDescription>
                                    </Alert>
                                )}
                                {!regions.data && !regions.isError && (
                                    <p role="status">正在加载地区资料…</p>
                                )}
                                {plan?.overLimit && (
                                    <Alert variant="warning">
                                        <AlertDescription>
                                            处理后共 {plan.allocations.length}{" "}
                                            个地址，超过 100 个，请移除多余组。
                                        </AlertDescription>
                                    </Alert>
                                )}
                                {allSkipped && (
                                    <Alert variant="info">
                                        <AlertDescription>
                                            解析 {rows.length}{" "}
                                            行，全部跳过，无需添加。原地址及数量保持不变。
                                        </AlertDescription>
                                    </Alert>
                                )}
                                {Boolean(plan?.conflicts) && (
                                    <Alert variant="warning">
                                        <AlertDescription>
                                            {plan?.conflicts}{" "}
                                            组数量冲突，请选择保持原数量、采用导入数量，或明确合并累加。
                                        </AlertDescription>
                                    </Alert>
                                )}
                                {exceedsDemand && plan?.changed ? (
                                    <Alert variant="warning">
                                        <AlertDescription>
                                            处理后配送总数量{" "}
                                            <QuantityValue
                                                value={total!}
                                                unit={unit}
                                            />
                                            ，超过需求{" "}
                                            <QuantityValue
                                                value={line.quantity}
                                                unit={unit}
                                            />
                                            。请核对累加是否符合实际需求；超量允许保存和提交采购，商品需求数量保持不变。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                                {plan && (
                                    <TableToolbarScope>
                                        <TableToolbar>
                                            <span>
                                                解析 {rows.length} 行 ·{" "}
                                                {plan.groups.length} 组地址 ·
                                                新增{" "}
                                                {plan.changes.additions.length}{" "}
                                                个 · 更新{" "}
                                                {plan.changes.updates.length} 个
                                                · 跳过 {plan.skipped} 行
                                                {total != null && (
                                                    <>
                                                        {" "}
                                                        · 处理后总数量{" "}
                                                        <QuantityValue
                                                            value={total}
                                                            unit={unit}
                                                        />
                                                    </>
                                                )}
                                            </span>
                                        </TableToolbar>
                                        <InquiryAddressImportGroups
                                            groups={plan.groups}
                                            id={id}
                                            unit={unit}
                                            disabled={disabled}
                                            onChoice={(group, choice) => {
                                                form.setFieldValue(
                                                    "choices",
                                                    (current) => ({
                                                        ...current,
                                                        [group.id]: choice,
                                                    }),
                                                )
                                                setFailure(null)
                                            }}
                                            onRemove={(group) => {
                                                const ids = new Set(
                                                    group.rows.map(
                                                        (row) => row.id,
                                                    ),
                                                )
                                                form.setFieldValue(
                                                    "rows",
                                                    (current) =>
                                                        current.filter(
                                                            (row) =>
                                                                !ids.has(
                                                                    row.id,
                                                                ),
                                                        ),
                                                )
                                                setFailure(null)
                                            }}
                                        />
                                    </TableToolbarScope>
                                )}
                                {progress && (
                                    <p className="text-sm text-muted-foreground">
                                        处理后：{progress.label}。
                                    </p>
                                )}
                                <p className="text-xs text-muted-foreground">
                                    相同收件资料按组处理。完全重复默认保留一条，与已有数量一致则跳过。“合并并累加数量”会计入本组所有粘贴行及原数量。收件资料有误时请返回修改文本。
                                </p>
                            </>
                        )}
                    </div>
                    <DialogFooter>
                        <Button
                            id={`${id}-cancel`}
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            {allSkipped ? "关闭" : "取消"}
                        </Button>
                        {rows.length > 0 && (
                            <Button
                                id={`${id}-back`}
                                type="button"
                                variant="outline"
                                onClick={() => {
                                    form.setFieldValue("rows", [])
                                    form.setFieldValue("choices", {})
                                    setFailure(null)
                                }}
                            >
                                返回修改文本
                            </Button>
                        )}
                        <Button
                            id={`${id}-confirm`}
                            type="submit"
                            disabled={
                                disabled ||
                                (rows.length > 0 &&
                                    (!ready ||
                                        plan?.overLimit ||
                                        !plan?.changed))
                            }
                        >
                            {!rows.length
                                ? "解析预览"
                                : allSkipped
                                  ? "无需添加"
                                  : plan?.changes.updates.length
                                    ? `确认新增 ${plan.changes.additions.length} 个、更新 ${plan.changes.updates.length} 个`
                                    : `确认添加 ${plan?.changes.additions.length ?? 0} 个地址`}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
