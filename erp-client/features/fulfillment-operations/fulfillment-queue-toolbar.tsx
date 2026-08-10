"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { FilterChip, ListToolbar, OptionCombobox } from "@/components/business"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
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

/**
 * W09 队列工具栏（第 1 / 2 层）。
 *
 * 第 0 层（scope / 类型）由页面 sticky 处理面渲染；本组件只负责：
 * - 第 1 层：搜索 + 主筛 ≤3（仓库 / 到期 / 门禁）
 * - 第 2 层：来源锁定 FilterChip
 * - actions：计数、清除、自动下一项
 */
export function FulfillmentQueueToolbar({
  q,
  warehouseId,
  due,
  gate,
  salesOrderId,
  purchaseOrderId,
  salesOrderNo,
  purchaseNo,
  autoNext,
  total,
  showAutoNext,
  type,
  onPatch,
  onAutoNextChange,
}: {
  q: string | undefined
  warehouseId: string | undefined
  due: DueFilter | undefined
  gate: GateFilter | undefined
  salesOrderId: string | undefined
  purchaseOrderId: string | undefined
  /** 内部 ID 只进 URL：chip 与摘要展示业务单号 */
  salesOrderNo: string | undefined
  purchaseNo: string | undefined
  autoNext: boolean
  total: number
  /** 只读角色不会连续处理，不显示自动下一项 */
  showAutoNext: boolean
  /** 任务类型筛选（slug，多值逗号分隔）；"all" 视为未激活 */
  type?: string | null
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
    q ||
      (type && type !== "all") ||
      warehouseId ||
      due ||
      gate ||
      salesOrderId ||
      purchaseOrderId
  )

  const hasChips = Boolean(salesOrderId || purchaseOrderId)

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
          <WarehouseSearchCombobox
            value={warehouseId}
            placeholder="仓库：全部"
            aria-label="按仓库筛选（只对入库和发货有效）"
            className="w-[9rem]"
            onValueChange={(value) =>
              onPatch({ warehouseId: value ?? null, currentWorkItemId: null })
            }
          />
          <OptionCombobox
            value={due ?? null}
            options={DUE_FILTER_OPTIONS}
            placeholder="到期：全部"
            size="sm"
            aria-label="按到期筛选"
            inputClassName="w-[8.5rem]"
            onValueChange={(v) =>
              onPatch({ due: v ?? null, currentWorkItemId: null })
            }
          />
          <OptionCombobox
            value={gate ?? null}
            options={GATE_FILTER_OPTIONS}
            placeholder="货款情况：全部"
            size="sm"
            aria-label="按货款情况筛选"
            inputClassName="w-[9.5rem]"
            onValueChange={(v) =>
              onPatch({ gate: v ?? null, currentWorkItemId: null })
            }
          />
        </>
      }
      secondary={
        hasChips ? (
          <>
            {salesOrderId ? (
              <FilterChip
                label={`销售单 ${salesOrderNo ?? "已定位"}`}
                onClear={() =>
                  onPatch({ salesOrderId: null, currentWorkItemId: null })
                }
              />
            ) : null}
            {purchaseOrderId ? (
              <FilterChip
                label={`采购单 ${purchaseNo ?? "已定位"}`}
                onClear={() =>
                  onPatch({ purchaseOrderId: null, currentWorkItemId: null })
                }
              />
            ) : null}
          </>
        ) : undefined
      }
      actions={
        <>
          <span className="text-xs text-muted-foreground" aria-live="polite">
            待处理 {total}
          </span>
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
        </>
      }
    />
  )
}
