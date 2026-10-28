"use client"

import * as React from "react"
import { OptionCombobox } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { DeliveryTemplateField } from "@/features/supplier-offerings/components/delivery-templates/delivery-template-field"
import type { useBatchWorkspace } from "../../hooks/use-batch-workspace"

export function BatchDetailsPanel({
    workspace,
    supplierId,
    categories,
}: {
    workspace: ReturnType<typeof useBatchWorkspace>
    supplierId: string
    categories: Array<{ value: string; label: string }>
}) {
    const { form, values } = workspace
    const [selected, setSelected] = React.useState<Set<string>>(new Set())
    const [unified, setUnified] = React.useState<string | null>(null)
    const clearable = [
        {
            wire: "input_tax_rate",
            field: "inputTaxPercentage",
            label: "进项税率",
        },
        { wire: "valid_from", field: "validFrom", label: "生效日期" },
        { wire: "valid_to", field: "validTo", label: "失效日期" },
        {
            wire: "delivery_template_id",
            field: "deliveryTemplateId",
            label: "配送模板（恢复供应商默认）",
        },
    ] as const
    const missing = values.categoryMappings.filter((row) => !row.categoryId)
    return (
        <div className="min-w-0 space-y-6">
            <section
                className="min-w-0 space-y-3 rounded-lg border p-4"
                aria-labelledby="supplier-catalog-batch-category-title"
            >
                <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
                    <h2
                        id="supplier-catalog-batch-category-title"
                        className="text-base font-semibold"
                    >
                        公司分类映射
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        新建商品使用本次映射；已有商品沿用原分类、品牌与类型。
                    </p>
                </div>
                <div className="flex flex-wrap items-center gap-3 border-t pt-3">
                    <span className="text-sm">将选中分类统一设为</span>
                    <OptionCombobox
                        id="supplier-catalog-batch-unified-category"
                        options={categories}
                        value={unified}
                        onValueChange={setUnified}
                        placeholder="选择公司分类"
                        className="w-72"
                    />
                    <Button
                        id="supplier-catalog-batch-apply-unified-category"
                        type="button"
                        variant="secondary"
                        disabled={!unified || selected.size === 0}
                        onClick={() => {
                            form.setFieldValue("clearFields", (current) =>
                                current.filter(
                                    (field) => field !== "category_id",
                                ),
                            )
                            form.setFieldValue("categoryMappings", (current) =>
                                current.map((row) =>
                                    selected.has(row.sourceId)
                                        ? { ...row, categoryId: unified ?? "" }
                                        : row,
                                ),
                            )
                        }}
                    >
                        应用至 {selected.size} 个分类
                    </Button>
                </div>
                <TableToolbarScope>
                    <TableToolbar>
                        <span>
                            {values.categoryMappings.length} 个来源分类 ·{" "}
                            {missing.length} 个待补
                        </span>
                    </TableToolbar>
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead className="w-10">
                                    <Checkbox
                                        id="supplier-catalog-batch-categories-select-page"
                                        aria-label="选择全部来源分类"
                                        checked={
                                            values.categoryMappings.length >
                                                0 &&
                                            selected.size ===
                                                values.categoryMappings.length
                                        }
                                        onCheckedChange={(checked) =>
                                            setSelected(
                                                checked
                                                    ? new Set(
                                                          values.categoryMappings.map(
                                                              (row) =>
                                                                  row.sourceId,
                                                          ),
                                                      )
                                                    : new Set(),
                                            )
                                        }
                                    />
                                </TableHead>
                                <TableHead>来源分类</TableHead>
                                <TableHead className="text-right">
                                    涉及 SKU
                                </TableHead>
                                <TableHead>公司分类</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {values.categoryMappings.map((row, index) => {
                                const key = toAutomationIdSegment(
                                    row.sourceId || "uncategorized",
                                )
                                return (
                                    <TableRow key={row.sourceId}>
                                        <TableCell>
                                            <Checkbox
                                                id={`supplier-catalog-batch-category-select-${key}`}
                                                aria-label={`选择${row.sourceName}`}
                                                checked={selected.has(
                                                    row.sourceId,
                                                )}
                                                onCheckedChange={(checked) =>
                                                    setSelected((current) => {
                                                        const next = new Set(
                                                            current,
                                                        )
                                                        if (checked)
                                                            next.add(
                                                                row.sourceId,
                                                            )
                                                        else
                                                            next.delete(
                                                                row.sourceId,
                                                            )
                                                        return next
                                                    })
                                                }
                                            />
                                        </TableCell>
                                        <TableCell>{row.sourceName}</TableCell>
                                        <TableCell className="num text-right">
                                            {row.count}
                                        </TableCell>
                                        <TableCell>
                                            <form.AppField
                                                name={`categoryMappings[${index}].categoryId`}
                                            >
                                                {(field) => (
                                                    <field.SelectField
                                                        id={`supplier-catalog-batch-category-map-${key}`}
                                                        label={`${row.sourceName}对应的公司分类`}
                                                        hideLabel
                                                        options={categories}
                                                        onValueChange={() =>
                                                            form.setFieldValue(
                                                                "clearFields",
                                                                (current) =>
                                                                    current.filter(
                                                                        (
                                                                            field,
                                                                        ) =>
                                                                            field !==
                                                                            "category_id",
                                                                    ),
                                                            )
                                                        }
                                                        placeholder="请选择公司分类"
                                                        className="min-w-56"
                                                    />
                                                )}
                                            </form.AppField>
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </TableToolbarScope>
                {missing.length ? (
                    <p className="text-sm text-warning">
                        {missing.length} 个来源分类尚未映射，涉及{" "}
                        {missing.reduce((total, row) => total + row.count, 0)}{" "}
                        个 SKU；可在核对页继续处理。
                    </p>
                ) : null}
            </section>
            <section
                className="space-y-4 rounded-lg border p-4"
                aria-labelledby="supplier-catalog-batch-terms-title"
            >
                <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
                    <h2
                        id="supplier-catalog-batch-terms-title"
                        className="text-base font-semibold"
                    >
                        供给条件
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        接口采购报价保持不变；公司售价不改变来源采购能力。
                    </p>
                </div>
                <div className="grid gap-5 border-t pt-4 lg:grid-cols-2">
                    <div className="space-y-4">
                        <form.AppField name="inputTaxPercentage">
                            {(field) => (
                                <field.TextField
                                    id="supplier-catalog-batch-input-tax"
                                    disabled={values.clearFields.includes(
                                        "input_tax_rate",
                                    )}
                                    label="进项税率（%）"
                                    inputMode="decimal"
                                    placeholder="请输入"
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="validFrom">
                            {(field) => (
                                <field.DateField
                                    id="supplier-catalog-batch-valid-from"
                                    disabled={values.clearFields.includes(
                                        "valid_from",
                                    )}
                                    label="生效日期"
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="validTo">
                            {(field) => (
                                <field.DateField
                                    id="supplier-catalog-batch-valid-to"
                                    disabled={values.clearFields.includes(
                                        "valid_to",
                                    )}
                                    label="失效日期"
                                    description="留空表示长期有效；明确清空请在单项调整中操作。"
                                />
                            )}
                        </form.AppField>
                    </div>
                    <form.AppField name="deliveryTemplateId">
                        {(field) => (
                            <DeliveryTemplateField
                                id="supplier-catalog-batch-delivery-template"
                                supplierId={supplierId}
                                value={field.state.value}
                                disabled={values.clearFields.includes(
                                    "delivery_template_id",
                                )}
                                onChange={field.handleChange}
                            />
                        )}
                    </form.AppField>
                </div>
                <details className="border-t pt-3">
                    <summary
                        id="supplier-catalog-batch-explicit-clears"
                        className="cursor-pointer text-sm"
                    >
                        明确清空批量字段
                    </summary>
                    <p className="py-3 text-xs text-muted-foreground">
                        未填写表示保持草稿值；勾选清空才移除可替换值。单项人工调整与明确清空继续受填值策略保护。
                    </p>
                    <div className="flex flex-wrap gap-4">
                        {clearable.map((item) => (
                            <label
                                key={item.wire}
                                htmlFor={`supplier-catalog-batch-clear-${item.wire}`}
                                className="flex items-center gap-2 text-sm"
                            >
                                <Checkbox
                                    id={`supplier-catalog-batch-clear-${item.wire}`}
                                    checked={values.clearFields.includes(
                                        item.wire,
                                    )}
                                    onCheckedChange={(checked) => {
                                        form.setFieldValue(
                                            "clearFields",
                                            (current) =>
                                                checked
                                                    ? [
                                                          ...current.filter(
                                                              (field) =>
                                                                  field !==
                                                                  item.wire,
                                                          ),
                                                          item.wire,
                                                      ]
                                                    : current.filter(
                                                          (field) =>
                                                              field !==
                                                              item.wire,
                                                      ),
                                        )
                                        if (checked)
                                            form.setFieldValue(item.field, "")
                                    }}
                                />
                                清空{item.label}
                            </label>
                        ))}
                        <label
                            htmlFor="supplier-catalog-batch-clear-categories"
                            className="flex items-center gap-2 text-sm"
                        >
                            <Checkbox
                                id="supplier-catalog-batch-clear-categories"
                                checked={values.clearFields.includes(
                                    "category_id",
                                )}
                                onCheckedChange={(checked) => {
                                    form.setFieldValue(
                                        "clearFields",
                                        (current) =>
                                            checked
                                                ? [
                                                      ...current.filter(
                                                          (field) =>
                                                              field !==
                                                              "category_id",
                                                      ),
                                                      "category_id",
                                                  ]
                                                : current.filter(
                                                      (field) =>
                                                          field !==
                                                          "category_id",
                                                  ),
                                    )
                                    if (checked)
                                        form.setFieldValue(
                                            "categoryMappings",
                                            (current) =>
                                                current.map((row) => ({
                                                    ...row,
                                                    categoryId: "",
                                                })),
                                        )
                                }}
                            />
                            清空本批新建商品分类
                        </label>
                    </div>
                </details>
                <p className="text-xs text-muted-foreground">
                    当前填值方式：
                    {values.overwrite ? "覆盖本批未入库项" : "仅填空值"} ·{" "}
                    {values.preserveManual
                        ? "保留单项人工调整与明确清空"
                        : "允许覆盖人工调整"}
                    。可返回设置售价，在更多设置中修改。
                </p>
            </section>
        </div>
    )
}
