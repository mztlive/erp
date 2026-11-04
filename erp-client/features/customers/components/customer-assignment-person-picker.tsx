"use client"

import * as React from "react"
import { UsersIcon } from "lucide-react"

import {
    PersonPickerDialog,
    type PersonPickerDepartment,
    type PersonPickerPerson,
} from "@/components/business/person-picker-dialog"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"

/** 负责销售分配沿用账号候选；组织查询仅补充部门，不增加候选人员。 */
export function CustomerAssignmentPersonPicker({
    id,
    value,
    onChange,
    onBlur,
    disabled,
    invalid,
    describedBy,
}: {
    id: string
    value: string
    onChange: (value: string) => void
    onBlur: () => void
    disabled?: boolean
    invalid?: boolean
    describedBy?: string
}) {
    const [open, setOpen] = React.useState(false)
    const owners = useOwnerOptionsQuery()
    const profile = useAccountProfileQuery()
    const canReadOrganization = hasPermission(
        profile.data?.permissions,
        "org_unit:list",
    )
    const organization = useOrganizationStateQuery(open && canReadOrganization)
    const view =
        canReadOrganization && !organization.isError
            ? organization.data
            : undefined
    const departments = React.useMemo<PersonPickerDepartment[]>(
        () =>
            (view?.units ?? []).map((unit) => ({
                id: unit.id,
                name: unit.enabled ? unit.name : `${unit.name}（已停用）`,
                parentId: unit.parent_id,
            })),
        [view],
    )
    const people = React.useMemo<PersonPickerPerson[]>(() => {
        if (owners.isError) return []
        const organizations = new Map(
            view?.people.map((person) => [person.id, person]),
        )
        return (owners.data ?? []).map((owner) => {
            const person = organizations.get(owner.userId)
            return {
                id: owner.userId,
                name: owner.displayName,
                description: owner.userCode,
                departmentId: person?.own_org_unit_id,
                disabled: person?.active === false,
                disabledReason:
                    person?.active === false ? "账号已停用" : undefined,
            }
        })
    }, [owners.data, owners.isError, view])
    const loading =
        owners.isFetching ||
        owners.isPending ||
        profile.isPending ||
        (canReadOrganization &&
            (organization.isPending || organization.isFetching))
    const error = owners.isError
        ? getErrorMessage(owners.error, "销售人员加载失败，请重试")
        : profile.isError
          ? "人员权限加载失败，请重试"
          : canReadOrganization && organization.isError
            ? getErrorMessage(organization.error, "部门加载失败，请重试")
            : undefined
    const selected = !owners.isError
        ? owners.data?.find((owner) => owner.userId === value)
        : undefined
    const pickerId = `${id}-picker`

    return (
        <>
            <Button
                id={id}
                type="button"
                variant="control"
                className="h-auto min-h-control w-full justify-between gap-3 py-2 text-left"
                disabled={disabled}
                aria-haspopup="dialog"
                aria-expanded={open}
                aria-controls={open ? `${pickerId}-dialog` : undefined}
                aria-invalid={invalid || undefined}
                aria-describedby={describedBy}
                onBlur={onBlur}
                onClick={() => {
                    setOpen(true)
                    void owners.refetch()
                }}
            >
                <span
                    className={cn(
                        "min-w-0 whitespace-normal break-words",
                        !value && "text-muted-foreground",
                    )}
                >
                    {value
                        ? selected
                            ? `${selected.displayName}${selected.userCode ? ` · ${selected.userCode}` : ""}`
                            : owners.isFetching
                              ? "正在核对已选人员…"
                              : "已选人员（请重新核对）"
                        : "选择销售人员"}
                </span>
                <UsersIcon aria-hidden="true" />
            </Button>
            <PersonPickerDialog
                id={pickerId}
                open={open && !disabled}
                onOpenChange={setOpen}
                mode="single"
                title="选择销售人员"
                description={
                    canReadOrganization
                        ? "按部门选择一位销售人员，确认后返回归属调整。"
                        : "当前账号未开放部门信息，可按姓名或账号选择。"
                }
                unassignedDepartmentLabel="未显示部门归属"
                value={value || null}
                onConfirm={(next) => {
                    onChange(next ?? "")
                    onBlur()
                }}
                people={people}
                departments={departments}
                loading={loading}
                error={error}
                onRetry={() => {
                    void owners.refetch()
                    void profile.refetch()
                    if (canReadOrganization) void organization.refetch()
                }}
            />
        </>
    )
}
