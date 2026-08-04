"use client"

import * as React from "react"

import { OptionCombobox, type ComboboxOption } from "@/components/business/option-combobox"

/**
 * 演示角色切换条（角色 + 可选权限演示 flag + 可选角色说明）。
 *
 * 覆盖原 4 处私有副本（supplier-settlements / supplier-api-connections /
 * history-backfill / import-opening）的差异：
 * - title：history-backfill 为「演示角色」，其余为「角色演示」
 * - roleOptions：各页角色清单不同
 * - demoFlag/flagOptions/onFlag：仅 supplier-settlements 与 supplier-api-connections 使用
 * - hintFor：尾部角色说明（history-backfill 不渲染）
 */
export type RoleDemoBarProps<T extends string, F extends string> = {
  /** 标题文案；默认「角色演示」 */
  title?: string
  role: T
  roleOptions: readonly ComboboxOption[]
  onRole: (role: T) => void
  /** 角色下拉宽度类 */
  roleClassName?: string
  /** 权限演示 flag（undefined 表示未启用） */
  demoFlag?: F
  flagOptions?: readonly ComboboxOption[]
  onFlag?: (flag: F | undefined) => void
  /** 权限下拉宽度类 */
  flagClassName?: string
  /** 尾部角色说明；不传则不渲染 */
  hintFor?: (role: T) => React.ReactNode
}

export function RoleDemoBar<T extends string, F extends string = string>({
  title = "角色演示",
  role,
  roleOptions,
  onRole,
  roleClassName,
  demoFlag,
  flagOptions,
  onFlag,
  flagClassName,
  hintFor,
}: RoleDemoBarProps<T, F>) {
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-xl border bg-muted/40 px-3 py-2 text-sm">
      <span className="text-muted-foreground">{title}</span>
      <OptionCombobox
        value={role}
        onValueChange={(v) => {
          if (v == null) return
          onRole(v as T)
        }}
        options={roleOptions}
        className={roleClassName}
        size="sm"
        allowClear={false}
      />
      {flagOptions && onFlag ? (
        <OptionCombobox
          value={demoFlag ?? "normal"}
          onValueChange={(v) => {
            if (v == null || v === "normal") onFlag(undefined)
            else onFlag(v as F)
          }}
          options={flagOptions}
          className={flagClassName}
          size="sm"
          allowClear={false}
        />
      ) : null}
      {hintFor ? (
        <span className="text-xs text-muted-foreground">{hintFor(role)}</span>
      ) : null}
    </div>
  )
}
