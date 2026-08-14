"use client"

import { OptionCombobox } from "@/components/business"
import { Input } from "@/components/ui/input"

type AuditFiltersProps = {
    fromParam?: string
    toParam?: string
    action?: string
    patchFilterUrl: (patch: Record<string, string | null | undefined>) => void
}

function AuditFilters({
    fromParam,
    toParam,
    action,
    patchFilterUrl,
}: AuditFiltersProps) {
    return (
        <>
            <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                起始
                <Input
                    type="date"
                    className="h-8 w-36"
                    value={fromParam ?? ""}
                    onChange={(e) =>
                        patchFilterUrl({
                            from: e.target.value || null,
                        })
                    }
                    aria-label="审计起始日期"
                />
            </label>
            <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                截止
                <Input
                    type="date"
                    className="h-8 w-36"
                    value={toParam ?? ""}
                    min={fromParam}
                    onChange={(e) =>
                        patchFilterUrl({
                            to: e.target.value || null,
                        })
                    }
                    aria-label="审计截止日期"
                />
            </label>
            <OptionCombobox
                value={action ?? "all"}
                onValueChange={(v) =>
                    patchFilterUrl({
                        action: (v ?? "all") === "all" ? null : (v ?? "all"),
                    })
                }
                options={[
                    { value: "all", label: "全部动作" },
                    {
                        value: "UPDATE_ROLE_PERMISSIONS",
                        label: "修改模块权限",
                    },
                    {
                        value: "EMERGENCY_REVOKE_USER_ROLE",
                        label: "紧急撤权",
                    },
                    {
                        value: "UPDATE_FIELD_POLICY",
                        label: "修改字段策略",
                    },
                    {
                        value: "MANAGE_DATA_SCOPE",
                        label: "修改数据范围",
                    },
                    { value: "QUERY_AUDIT", label: "查询审计" },
                    {
                        value: "OPEN_SUPPLIER",
                        label: "打开供应商",
                    },
                    {
                        value: "EXPORT_RECEIVABLE",
                        label: "导出应收明细",
                    },
                    {
                        value: "CREATE_ADJUSTMENT",
                        label: "创建库存调整",
                    },
                    {
                        value: "VIEW_CUSTOMER_SENSITIVE",
                        label: "短时揭示敏感字段",
                    },
                    {
                        value: "PERMISSION_VERSION_BUMP",
                        label: "权限版本推进",
                    },
                ]}
                className="w-[10rem]"
                size="sm"
                allowClear={false}
                aria-label="动作"
                placeholder="全部动作"
            />
        </>
    )
}

export { AuditFilters }
