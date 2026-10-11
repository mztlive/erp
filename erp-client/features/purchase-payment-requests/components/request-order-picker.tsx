import { useState } from "react"
import { SearchIcon } from "lucide-react"
import { MoneyValue, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    Table,
    TableHeader,
    TableBody,
    TableRow,
    TableHead,
    TableCell,
} from "@/components/ui/table"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { groupKey, regroup, totalOf } from "../lib/draft"
import type { DraftForm } from "../hooks/use-draft-form"
import type { PaymentCandidate } from "../types"

export function RequestOrderPicker({
    form,
    candidates,
    editing,
}: {
    form: DraftForm
    candidates: PaymentCandidate[]
    editing: boolean
}) {
    const [search, setSearch] = useState("")
    const [supplierId, setSupplierId] = useState<string | null>(null)
    const suppliers = Array.from(
        new Map(
            candidates.map((item) => [item.supplier_id, item.supplier_name]),
        ),
        ([value, label]) => ({ value, label }),
    )
    const visible = candidates.filter(
        (item) =>
            (!supplierId || item.supplier_id === supplierId) &&
            [item.purchase_order_no, item.purchase_summary]
                .join(" ")
                .toLowerCase()
                .includes(search.trim().toLowerCase()),
    )
    return (
        <form.Subscribe selector={(state) => state.values.selectedIds}>
            {(ids) => {
                const selected = new Set(ids)
                const rows = candidates.filter((item) =>
                    selected.has(item.payable_account_id),
                )
                const groups = regroup(ids, candidates, [])
                const allChecked =
                    visible.length > 0 &&
                    visible.every((item) =>
                        selected.has(item.payable_account_id),
                    )
                return (
                    <div className="space-y-5">
                        <div className="flex flex-wrap gap-3">
                            <div className="relative min-w-0 flex-1 basis-72 sm:max-w-xl">
                                <SearchIcon className="absolute top-2.5 left-3 size-4 text-muted-foreground" />
                                <Input
                                    id="ppr-create-order-search"
                                    className="pl-9"
                                    value={search}
                                    onChange={(event) =>
                                        setSearch(event.target.value)
                                    }
                                    placeholder="搜索采购单号或采购内容"
                                    aria-label="搜索可申请采购单"
                                />
                            </div>
                            <OptionCombobox
                                id="ppr-create-supplier-filter"
                                options={suppliers}
                                value={supplierId}
                                onValueChange={setSupplierId}
                                filterLabel="供应商"
                                placeholder="全部供应商"
                                emptyLabel="没有符合条件的供应商"
                                aria-label="筛选供应商"
                                className="w-full sm:w-80"
                            />
                        </div>
                        <p className="text-xs text-muted-foreground">
                            {editing
                                ? "修改已有草稿仅可选择原供应商、原付款主体的采购单。"
                                : "支持跨供应商选择；仅显示已生效、有可申请余额且你有申请资格的采购单。"}
                        </p>
                        <div className="overflow-x-auto rounded-lg border">
                            <Table className="w-full text-left text-sm">
                                <TableHeader className="bg-muted/60 text-xs text-muted-foreground">
                                    <TableRow>
                                        <TableHead className="w-10 p-3">
                                            <NativeCheckbox
                                                id="ppr-create-select-visible"
                                                aria-label="选择当前筛选结果"
                                                checked={allChecked}
                                                indeterminate={
                                                    !allChecked &&
                                                    visible.some((item) =>
                                                        selected.has(
                                                            item.payable_account_id,
                                                        ),
                                                    )
                                                }
                                                disabled={!visible.length}
                                                onCheckedChange={(checked) => {
                                                    const next = new Set(ids)
                                                    for (const item of visible) {
                                                        if (checked)
                                                            next.add(
                                                                item.payable_account_id,
                                                            )
                                                        else
                                                            next.delete(
                                                                item.payable_account_id,
                                                            )
                                                    }
                                                    form.setFieldValue(
                                                        "selectedIds",
                                                        [...next],
                                                    )
                                                }}
                                            />
                                        </TableHead>
                                        <TableHead className="p-3 font-medium">
                                            采购单 / 采购内容
                                        </TableHead>
                                        <TableHead className="p-3 font-medium">
                                            供应商 / 我方付款主体
                                        </TableHead>
                                        <TableHead className="p-3 text-right font-medium whitespace-nowrap">
                                            可申请余额
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {visible.map((item) => (
                                        <TableRow
                                            key={item.payable_account_id}
                                            id={`ppr-select-row-${toAutomationIdSegment(item.payable_account_id)}`}
                                            className="cursor-pointer border-t outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
                                            tabIndex={0}
                                            aria-selected={selected.has(
                                                item.payable_account_id,
                                            )}
                                            data-state={
                                                selected.has(
                                                    item.payable_account_id,
                                                )
                                                    ? "selected"
                                                    : undefined
                                            }
                                            onClick={(event) => {
                                                if (
                                                    (
                                                        event.target as HTMLElement
                                                    ).closest(
                                                        "input, button, a, label",
                                                    )
                                                )
                                                    return
                                                event.currentTarget
                                                    .querySelector<HTMLInputElement>(
                                                        'input[type="checkbox"]',
                                                    )
                                                    ?.click()
                                            }}
                                            onKeyDown={(event) => {
                                                if (
                                                    event.target !==
                                                        event.currentTarget ||
                                                    (event.key !== " " &&
                                                        event.key !== "Enter")
                                                )
                                                    return
                                                event.preventDefault()
                                                event.currentTarget
                                                    .querySelector<HTMLInputElement>(
                                                        'input[type="checkbox"]',
                                                    )
                                                    ?.click()
                                            }}
                                        >
                                            <TableCell className="p-3">
                                                <NativeCheckbox
                                                    id={`ppr-select-${toAutomationIdSegment(item.payable_account_id)}`}
                                                    aria-label={`选择采购单 ${item.purchase_order_no}`}
                                                    checked={selected.has(
                                                        item.payable_account_id,
                                                    )}
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        form.setFieldValue(
                                                            "selectedIds",
                                                            checked
                                                                ? [
                                                                      ...ids,
                                                                      item.payable_account_id,
                                                                  ]
                                                                : ids.filter(
                                                                      (id) =>
                                                                          id !==
                                                                          item.payable_account_id,
                                                                  ),
                                                        )
                                                    }
                                                />
                                            </TableCell>
                                            <TableCell className="max-w-80 p-3">
                                                <label
                                                    htmlFor={`ppr-select-${toAutomationIdSegment(item.payable_account_id)}`}
                                                    className="num cursor-pointer font-medium break-all"
                                                >
                                                    {item.purchase_order_no}
                                                </label>
                                                <p className="mt-1 text-xs text-muted-foreground">
                                                    {item.purchase_summary ||
                                                        item.payment_term_name}
                                                </p>
                                            </TableCell>
                                            <TableCell className="p-3">
                                                <p>{item.supplier_name}</p>
                                                <p className="mt-1 text-xs text-muted-foreground">
                                                    {item.our_company_name}
                                                </p>
                                            </TableCell>
                                            <TableCell className="p-3 text-right">
                                                <MoneyValue
                                                    value={
                                                        item.available_amount
                                                    }
                                                />
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                            {!visible.length && (
                                <p className="px-4 py-12 text-center text-muted-foreground">
                                    {candidates.length
                                        ? "没有匹配的采购单，请调整搜索条件。"
                                        : "暂无可申请付款的采购单。请确认采购已生效且仍有可申请余额。"}
                                </p>
                            )}
                        </div>
                        <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-muted/50 p-4">
                            <div>
                                <p className="font-medium">
                                    已选 {rows.length} 张采购单
                                </p>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    涉及{" "}
                                    {
                                        new Set(
                                            rows.map(
                                                (item) => item.supplier_id,
                                            ),
                                        ).size
                                    }{" "}
                                    家供应商、
                                    {
                                        new Set(
                                            rows.map(
                                                (item) => item.our_company_id,
                                            ),
                                        ).size
                                    }{" "}
                                    个我方付款主体，将按付款关系分为{" "}
                                    {groups.length} 组。
                                </p>
                            </div>
                            <div className="text-right">
                                <p className="text-xs text-muted-foreground">
                                    可申请余额合计
                                </p>
                                <MoneyValue
                                    value={totalOf(
                                        rows.map(
                                            (item) => item.available_amount,
                                        ),
                                    )}
                                />
                                <p className="mt-1 text-xs text-muted-foreground">
                                    本次申请金额在下一步填写
                                </p>
                            </div>
                        </div>
                        {!!rows.length && (
                            <section className="space-y-2">
                                <div className="flex items-center justify-between">
                                    <h3 className="font-medium">分组预览</h3>
                                    <Button
                                        id="ppr-create-clear-selection"
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        onClick={() =>
                                            form.setFieldValue(
                                                "selectedIds",
                                                [],
                                            )
                                        }
                                    >
                                        清空选择
                                    </Button>
                                </div>
                                {groups.map((group) => {
                                    const first = rows.find(
                                        (item) => groupKey(item) === group.key,
                                    )!
                                    return (
                                        <div
                                            key={group.key}
                                            className="flex flex-wrap justify-between gap-2 rounded-lg border px-4 py-3"
                                        >
                                            <span>
                                                {first.supplier_name}
                                                <span className="mx-3 text-muted-foreground">
                                                    ←
                                                </span>
                                                {first.our_company_name}
                                            </span>
                                            <span className="text-muted-foreground">
                                                {group.ids.length} 张采购单
                                            </span>
                                        </div>
                                    )
                                })}
                            </section>
                        )}
                    </div>
                )
            }}
        </form.Subscribe>
    )
}
