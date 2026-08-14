"use client"

import { OptionCombobox } from "@/components/business"

type ConfigFiltersProps = {
    org?: string
    status?: string
    risk?: string
    orgOptions: { value: string; label: string }[]
    patchFilterUrl: (patch: Record<string, string | null | undefined>) => void
}

function ConfigFilters({
    org,
    status,
    risk,
    orgOptions,
    patchFilterUrl,
}: ConfigFiltersProps) {
    return (
        <>
            <OptionCombobox
                value={org ?? "all"}
                onValueChange={(v) =>
                    patchFilterUrl({
                        org: (v ?? "all") === "all" ? null : (v ?? "all"),
                    })
                }
                options={[
                    { value: "all", label: "全部组织" },
                    ...orgOptions,
                ]}
                className="w-[9rem]"
                size="sm"
                allowClear={false}
                aria-label="组织"
                placeholder="全部组织"
            />
            <OptionCombobox
                value={status ?? "all"}
                onValueChange={(v) =>
                    patchFilterUrl({
                        status: (v ?? "all") === "all" ? null : (v ?? "all"),
                    })
                }
                options={[
                    { value: "all", label: "全部状态" },
                    { value: "enabled", label: "启用" },
                    { value: "disabled", label: "停用" },
                ]}
                className="w-[8rem]"
                size="sm"
                allowClear={false}
                aria-label="状态"
                placeholder="全部状态"
            />
            <OptionCombobox
                value={risk ?? "all"}
                onValueChange={(v) =>
                    patchFilterUrl({
                        risk: (v ?? "all") === "all" ? null : (v ?? "all"),
                    })
                }
                options={[
                    { value: "all", label: "全部风险" },
                    {
                        value: "HIGH_PRIVILEGE",
                        label: "高权限",
                    },
                    {
                        value: "EMPTY_SCOPE",
                        label: "空数据范围",
                    },
                    {
                        value: "EXPIRING_SOON",
                        label: "即将过期",
                    },
                    {
                        value: "ACCESS_ADMIN",
                        label: "权限管理",
                    },
                ]}
                className="w-[9rem]"
                size="sm"
                allowClear={false}
                aria-label="权限风险"
                placeholder="全部风险"
            />
        </>
    )
}

export { ConfigFilters }
