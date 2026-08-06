"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { ArrowLeftIcon } from "lucide-react"
import { z } from "zod"

import {
  BusinessFailureState,
  PageHeader,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import {
  PermissionOptionsPanel,
} from "@/features/admin/permission-panel"
import {
  useRoleMutations,
  useRolesQuery,
} from "@/features/admin/queries"

type RoleFormValues = {
  name: string
  permissions: string[]
}

const roleFormSchema = z.object({
  name: z
    .string()
    .trim()
    .min(2, "角色名称长度必须在2-32个字符之间")
    .max(32, "角色名称长度必须在2-32个字符之间"),
  permissions: z.array(z.string()),
})

/** 角色返回列表地址（权限与审计 · 角色视图）。 */
const ROLES_LIST_HREF = "/system/access-audit?view=roles"

/**
 * 角色新建 / 编辑整页表单。
 *
 * 权限目录有 35 组 / 372 项，弹窗放不下，独立整页承载：
 * 名称字段 + 可搜索、可折叠、组内全选的权限面板。
 *
 * @param roleId 编辑目标角色 ID；null 表示新建。
 */
export function RoleFormPage({ roleId }: { roleId: string | null }) {
  const router = useRouter()
  const rolesQuery = useRolesQuery()
  const { createRole, updateRole, isCreating, isUpdating } = useRoleMutations()
  const [submitError, setSubmitError] = React.useState<string | null>(null)

  const isEdit = roleId !== null
  const role = isEdit
    ? rolesQuery.data?.find((candidate) => candidate.id === roleId) ?? null
    : null
  const pending = isCreating || isUpdating

  const form = useAppForm({
    defaultValues: {
      name: role?.name ?? "",
      permissions: role?.permissions ?? [],
    } satisfies RoleFormValues,
    validators: { onChange: roleFormSchema },
    onSubmit: async ({ value }) => {
      setSubmitError(null)
      try {
        if (isEdit && roleId) {
          await updateRole({
            id: roleId,
            payload: {
              name: value.name.trim(),
              permissions: value.permissions,
            },
          })
        } else {
          await createRole({
            name: value.name.trim(),
            permissions: value.permissions,
          })
        }
        router.push(ROLES_LIST_HREF)
      } catch (error) {
        setSubmitError(
          error instanceof Error ? error.message : "操作失败，请重试。"
        )
      }
    },
  })

  // 编辑模式：角色列表加载中
  if (isEdit && rolesQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-16 animate-pulse rounded-xl bg-muted" />
        <div className="h-96 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  // 编辑模式：角色列表加载失败
  if (isEdit && rolesQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="编辑角色"
          breadcrumbs={[
            { id: "system", label: "系统", href: "/system/access-audit" },
            {
              id: "access",
              label: "权限与审计",
              href: "/system/access-audit?view=roles",
            },
            { id: "edit", label: "编辑角色", current: true },
          ]}
        />
        <BusinessFailureState
          kind="system"
          title="角色信息加载失败"
          description="请重试。"
          action={
            <Button
              type="button"
              variant="outline"
              onClick={() => void rolesQuery.refetch()}
            >
              重试
            </Button>
          }
        />
      </div>
    )
  }

  // 编辑模式：角色不存在
  if (isEdit && !role) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="编辑角色"
          breadcrumbs={[
            { id: "system", label: "系统", href: "/system/access-audit" },
            {
              id: "access",
              label: "权限与审计",
              href: "/system/access-audit?view=roles",
            },
            { id: "edit", label: "编辑角色", current: true },
          ]}
        />
        <BusinessFailureState
          kind="system"
          title="未找到角色"
          description="该角色不存在或已被删除，可返回角色列表重新选择。"
          action={
            <Button type="button" variant="outline" onClick={() => router.push(ROLES_LIST_HREF)}>
              返回角色列表
            </Button>
          }
        />
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
      <PageHeader
        title={isEdit ? "编辑角色" : "新建角色"}
        description={
          isEdit
            ? "调整角色名称与权限策略；保存后立即对绑定该角色的账号生效。"
            : "创建角色并配置权限策略；权限决定该角色可访问的工作面与动作。"
        }
        breadcrumbs={[
          { id: "system", label: "系统", href: "/system/access-audit" },
          {
            id: "access",
            label: "权限与审计",
            href: "/system/access-audit?view=roles",
          },
          { id: "form", label: isEdit ? "编辑角色" : "新建角色", current: true },
        ]}
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={pending}
            onClick={() => router.push(ROLES_LIST_HREF)}
          >
            <ArrowLeftIcon className="size-4" aria-hidden="true" />
            返回角色列表
          </Button>
        }
      />

      <Card>
        <CardHeader>
          <CardTitle>角色配置</CardTitle>
          <CardDescription>
            角色名称为 2-32 个字符；系统内置角色不可修改或删除。权限按模块分组勾选，可搜索过滤。
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault()
              void form.handleSubmit()
            }}
          >
            <form.AppField
              name="name"
              children={(field) => (
                <field.TextField
                  label="角色名称"
                  placeholder="如：销售经理"
                  className="max-w-md"
                />
              )}
            />
            <form.AppField
              name="permissions"
              mode="array"
              children={(field) => {
                const selected = field.state.value ?? []
                const isInvalid =
                  field.state.meta.isTouched && !field.state.meta.isValid
                const errors = toFieldErrors(field.state.meta.errors)
                return (
                  <Field data-invalid={isInvalid || undefined}>
                    <FieldLabel>权限</FieldLabel>
                    <PermissionOptionsPanel
                      selected={selected}
                      onChange={(next) => {
                        field.handleChange(next)
                        form.validateField("permissions", "change")
                      }}
                    />
                    <p className="text-xs text-muted-foreground">
                      未选择任何权限的角色仅保留基础登录能力，可在创建后再次编辑。
                    </p>
                    {isInvalid ? <FieldError errors={errors} /> : null}
                  </Field>
                )
              }}
            />
            {submitError ? (
              <Alert variant="destructive" role="alert">
                <AlertTitle>提交失败</AlertTitle>
                <AlertDescription>{submitError}</AlertDescription>
              </Alert>
            ) : null}
            <CardFooter className="flex justify-end gap-2 border-t px-0 pb-0 pt-4">
              <Button
                type="button"
                variant="outline"
                disabled={pending}
                onClick={() => router.push(ROLES_LIST_HREF)}
              >
                取消
              </Button>
              <form.AppForm>
                <form.SubmitButton label={isEdit ? "保存" : "创建"} />
              </form.AppForm>
            </CardFooter>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
