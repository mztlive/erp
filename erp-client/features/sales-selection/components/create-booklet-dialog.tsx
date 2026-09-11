"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { useCreateBookletMutation } from "@/features/sales-selection/queries"
import type {
    CreateTierInput,
    PoolFilterSnapshot,
    PoolSourceKind,
    SelectionForm,
    SubmitMode,
} from "@/features/sales-selection/types"

export function CreateBookletDialog({
    open,
    onOpenChange,
    poolSourceKind,
    poolFilter,
    skuIds,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    poolSourceKind: PoolSourceKind
    poolFilter?: PoolFilterSnapshot
    skuIds?: string[]
}) {
    const router = useRouter()
    const create = useCreateBookletMutation()
    const [customerId, setCustomerId] = React.useState("")
    const [form, setForm] = React.useState<SelectionForm>("SINGLE_SKU")
    const [submitMode, setSubmitMode] =
        React.useState<SubmitMode>("BY_QUANTITY")
    const [tiers, setTiers] = React.useState<CreateTierInput[]>([
        {
            name: "100 元档",
            target_amount: "100.00",
            tolerance: "5.00",
            expected_count: 4,
            sku_count: 3,
        },
    ])
    const idempotencyKey = React.useRef(crypto.randomUUID())

    const onSubmit = async () => {
        const booklet = await create.mutateAsync({
            idempotencyKey: idempotencyKey.current,
            customerId,
            form,
            submitMode,
            poolSourceKind,
            poolFilter: poolSourceKind === "FILTER" ? poolFilter : undefined,
            skuIds: poolSourceKind === "SELECTION" ? skuIds : undefined,
            tiers: form === "PACKAGE" ? tiers : [],
        })
        idempotencyKey.current = crypto.randomUUID()
        onOpenChange(false)
        router.push(`/sales/selections/${booklet.id}`)
    }

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                id="sales-selection-create-dialog"
                className="max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>发起选品</DialogTitle>
                </DialogHeader>
                <div className="grid gap-4">
                    <Field>
                        <FieldLabel>客户</FieldLabel>
                        <CustomerSearchCombobox
                            value={customerId}
                            onValueChange={(id) => setCustomerId(id ?? "")}
                        />
                    </Field>
                    <Field>
                        <FieldLabel>选品形态</FieldLabel>
                        <select
                            id="sales-selection-create-form"
                            className="h-9 rounded-md border bg-background px-3 text-sm"
                            value={form}
                            onChange={(event) =>
                                setForm(event.target.value as SelectionForm)
                            }
                        >
                            <option value="SINGLE_SKU">单品</option>
                            <option value="PACKAGE">套餐</option>
                        </select>
                    </Field>
                    <Field>
                        <FieldLabel>提交方式</FieldLabel>
                        <select
                            id="sales-selection-create-submit-mode"
                            className="h-9 rounded-md border bg-background px-3 text-sm"
                            value={submitMode}
                            onChange={(event) =>
                                setSubmitMode(event.target.value as SubmitMode)
                            }
                        >
                            <option value="BY_QUANTITY">按份采购</option>
                            <option value="MALL_REDEEM">商城兑换</option>
                        </select>
                    </Field>
                    {form === "PACKAGE"
                        ? tiers.map((tier, index) => (
                              <div
                                  key={index}
                                  className="grid grid-cols-2 gap-2 rounded-lg border p-3"
                              >
                                  <Input
                                      id={`sales-selection-tier-name-${index}`}
                                      value={tier.name}
                                      onChange={(event) => {
                                          const next = [...tiers]
                                          next[index] = {
                                              ...tier,
                                              name: event.target.value,
                                          }
                                          setTiers(next)
                                      }}
                                      placeholder="档位名称"
                                  />
                                  <Input
                                      id={`sales-selection-tier-target-${index}`}
                                      value={tier.target_amount}
                                      onChange={(event) => {
                                          const next = [...tiers]
                                          next[index] = {
                                              ...tier,
                                              target_amount: event.target.value,
                                          }
                                          setTiers(next)
                                      }}
                                      placeholder="目标金额"
                                  />
                                  <Input
                                      id={`sales-selection-tier-tolerance-${index}`}
                                      value={tier.tolerance}
                                      onChange={(event) => {
                                          const next = [...tiers]
                                          next[index] = {
                                              ...tier,
                                              tolerance: event.target.value,
                                          }
                                          setTiers(next)
                                      }}
                                      placeholder="容差"
                                  />
                                  <Input
                                      id={`sales-selection-tier-count-${index}`}
                                      type="number"
                                      value={tier.expected_count}
                                      onChange={(event) => {
                                          const next = [...tiers]
                                          next[index] = {
                                              ...tier,
                                              expected_count: Number(
                                                  event.target.value,
                                              ),
                                          }
                                          setTiers(next)
                                      }}
                                      placeholder="套餐数量"
                                  />
                              </div>
                          ))
                        : null}
                    <p className="text-sm text-muted-foreground">
                        {poolSourceKind === "SELECTION"
                            ? `将按勾选的 ${skuIds?.length ?? 0} 件商品准备选品册。`
                            : "将按当前筛选条件准备选品册，不使用当前页已加载行。"}
                    </p>
                </div>
                <DialogFooter>
                    <Button
                        id="sales-selection-create-submit"
                        type="button"
                        disabled={!customerId || create.isPending}
                        onClick={() => void onSubmit()}
                    >
                        {create.isPending ? "创建中…" : "创建并准备"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
