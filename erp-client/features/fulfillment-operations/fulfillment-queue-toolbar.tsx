"use client"

import * as React from "react"
import { SearchIcon, XIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import {
  DUE_FILTER_OPTIONS,
  GATE_FILTER_OPTIONS,
  type DueFilter,
  type GateFilter,
} from "./filters"

export type QueueFilterPatch = Record<string, string | null>

const SCOPE_OPTIONS = [
  { value: "mine", label: "仅我的" },
  { value: "role_pool", label: "全组" },
]

/**
 * W09 队列工具栏。
 *
 * 只暴露 mock api 真正参与过滤的维度（单号、仓库、到期、门禁），
 * 以及来源页带入的对象筛选 —— 后者做成可移除的标记，避免 URL 里留下界面改不动的隐形状态。
 */
export function FulfillmentQueueToolbar({
  q,
  warehouseId,
  warehouseOptions,
  due,
  gate,
  salesOrderId,
  purchaseOrderId,
  salesOrderNo,
  purchaseNo,
  autoNext,
  total,
  scope,
  showScope,
  showAutoNext,
  roleValue,
  roleOptions,
  devSimulation,
  onPatch,
  onAutoNextChange,
}: {
  q: string | undefined
  warehouseId: string | undefined
  warehouseOptions: ReadonlyArray<{ value: string; label: string }>
  due: DueFilter | undefined
  gate: GateFilter | undefined
  salesOrderId: string | undefined
  purchaseOrderId: string | undefined
  /** 内部 ID 只进 URL：chip 与摘要展示业务单号 */
  salesOrderNo: string | undefined
  purchaseNo: string | undefined
  autoNext: boolean
  total: number
  scope: "mine" | "role_pool"
  /** 只读角色没有「我的」概念，不显示范围切换 */
  showScope: boolean
  /** 只读角色不会连续处理，不显示自动下一项 */
  showAutoNext: boolean
  roleValue: string
  roleOptions: ReadonlyArray<{ value: string; label: string }>
  devSimulation?: React.ReactNode
  onPatch: (patch: QueueFilterPatch) => void
  onAutoNextChange: (next: boolean) => void
}) {
  // 输入过程不打 URL，回车/失焦才提交，避免每个按键都重查队列
  const [searchDraft, setSearchDraft] = React.useState(q ?? "")
  React.useEffect(() => {
    setSearchDraft(q ?? "")
  }, [q])

  const commitSearch = () => {
    const next = searchDraft.trim()
    if (next === (q ?? "")) return
    onPatch({ q: next || null, currentWorkItemId: null })
  }

  const hasFilters = Boolean(
    q || warehouseId || due || gate || salesOrderId || purchaseOrderId
  )

  return (
    <ListToolbar
      aria-label="待办筛选"
      search={
        <form
          onSubmit={(event) => {
            event.preventDefault()
            commitSearch()
          }}
        >
          <InputGroup>
            <InputGroupAddon>
              <SearchIcon aria-hidden="true" />
            </InputGroupAddon>
            <InputGroupInput
              value={searchDraft}
              onChange={(event) => setSearchDraft(event.target.value)}
              onBlur={commitSearch}
              placeholder="销售单号、采购单号、客户、供应商"
              aria-label="搜索任务"
            />
          </InputGroup>
        </form>
      }
      filters={
        <>
          {showScope ? (
            <OptionCombobox
              value={scope}
              options={SCOPE_OPTIONS}
              allowClear={false}
              aria-label="看谁的任务"
              onValueChange={(v) =>
                onPatch({ scope: v ?? "mine", currentWorkItemId: null })
              }
            />
          ) : null}
          {warehouseOptions.length > 0 ? (
            <OptionCombobox
              value={warehouseId ?? null}
              options={warehouseOptions}
              placeholder="仓库：全部"
              aria-label="按仓库筛选（只对入库和发货有效）"
              onValueChange={(v) =>
                onPatch({ warehouseId: v ?? null, currentWorkItemId: null })
              }
            />
          ) : null}
          <OptionCombobox
            value={due ?? null}
            options={DUE_FILTER_OPTIONS}
            placeholder="到期：全部"
            aria-label="按到期筛选"
            onValueChange={(v) =>
              onPatch({ due: v ?? null, currentWorkItemId: null })
            }
          />
          <OptionCombobox
            value={gate ?? null}
            options={GATE_FILTER_OPTIONS}
            placeholder="货款情况：全部"
            aria-label="按货款情况筛选"
            onValueChange={(v) =>
              onPatch({ gate: v ?? null, currentWorkItemId: null })
            }
          />
          {salesOrderId ? (
            <ObjectFilterChip
              label={`销售单 ${salesOrderNo ?? "已定位"}`}
              onClear={() =>
                onPatch({ salesOrderId: null, currentWorkItemId: null })
              }
            />
          ) : null}
          {purchaseOrderId ? (
            <ObjectFilterChip
              label={`采购单 ${purchaseNo ?? "已定位"}`}
              onClear={() =>
                onPatch({ purchaseOrderId: null, currentWorkItemId: null })
              }
            />
          ) : null}
          {hasFilters ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() =>
                onPatch({
                  type: null,
                  q: null,
                  warehouseId: null,
                  due: null,
                  gate: null,
                  salesOrderId: null,
                  purchaseOrderId: null,
                  currentWorkItemId: null,
                })
              }
            >
              清除筛选
            </Button>
          ) : null}
        </>
      }
      actions={
        <>
          <span className="text-xs text-muted-foreground" aria-live="polite">
            待处理 {total}
          </span>
          <OptionCombobox
            value={roleValue}
            options={roleOptions}
            allowClear={false}
            aria-label="当前身份（演示）"
            onValueChange={(v) =>
              onPatch({
                demoRole: v ?? null,
                // 换身份后可见类型变了，旧的类型/单号筛选和游标一律作废
                type: null,
                currentWorkItemId: null,
                warehouseId: null,
                salesOrderId: null,
                purchaseOrderId: null,
              })
            }
          />
          {showAutoNext ? (
            <div className="flex items-center gap-2">
              <Label htmlFor="ff-auto-next" className="text-muted-foreground">
                自动下一项
              </Label>
              <Switch
                id="ff-auto-next"
                checked={autoNext}
                onCheckedChange={onAutoNextChange}
              />
            </div>
          ) : null}
          {devSimulation}
        </>
      }
    />
  )
}

function ObjectFilterChip({
  label,
  onClear,
}: {
  label: string
  onClear: () => void
}) {
  return (
    <Badge variant="secondary" className="gap-1 font-normal">
      {label}
      <button
        type="button"
        onClick={onClear}
        aria-label={`清除${label}筛选`}
        className="rounded-sm opacity-70 hover:opacity-100"
      >
        <XIcon className="size-3" aria-hidden="true" />
      </button>
    </Badge>
  )
}
