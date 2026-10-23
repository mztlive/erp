"use client"

import { useFieldContext } from "@/components/form/form-context"
import { MultiSelectField } from "@/components/form/multi-select-field"
import {
    SelectField,
    type SelectFieldOption,
} from "@/components/form/select-field"
import { ASSIGNMENT_STRATEGY_LABEL } from "./model"

type OwnerPoolFieldProps = {
    id: string
    options: readonly SelectFieldOption[]
    selectedNames?: Readonly<Record<string, string>>
    onRefresh: () => Promise<unknown>
    description?: string
}

/** 负责人池统一编辑入口；候选刷新保留成员及轮询顺序。 */
export function OwnerPoolField({
    id,
    options,
    selectedNames = {},
    onRefresh,
    description,
}: OwnerPoolFieldProps) {
    const field = useFieldContext<string[]>()
    const poolOptions = [
        ...options,
        ...field.state.value
            .filter((id) => !options.some((option) => option.value === id))
            .map((id) => ({
                value: id,
                label: `${selectedNames[id] ?? "已选负责人"}（当前不可用）`,
                disabled: true,
            })),
    ]
    return (
        <MultiSelectField
            id={id}
            label="负责人池"
            required
            options={poolOptions}
            onRefresh={onRefresh}
            placeholder="选择负责人，可多选"
            emptyLabel="没有符合资格的负责人，请先配置角色权限"
            description={
                description ??
                "按选择顺序轮流分派，每个新任务分配给一人。需要调整顺序时，移除后重新选择。"
            }
        />
    )
}

/** 采购与财务使用同一已实现策略清单。 */
export function AssignmentStrategyField({ id }: { id: string }) {
    return (
        <SelectField
            id={id}
            label="分配方式"
            required
            allowClear={false}
            options={Object.entries(ASSIGNMENT_STRATEGY_LABEL).map(
                ([value, label]) => ({ value, label }),
            )}
        />
    )
}
