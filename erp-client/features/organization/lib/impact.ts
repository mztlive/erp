import type { BusinessDiffEntry } from "@/components/business"
import { KIND_LABEL, OPERATION_LABEL } from "@/features/organization/lib/labels"
import {
    personLabel as resolvePerson,
    roleLabel as resolveRole,
    unitLabel,
} from "@/features/organization/lib/tree"
import type {
    OrganizationChangeReceipt,
    OrganizationState,
    OrganizationStateView,
    OrgUnit,
} from "@/features/organization/types"

function unitMap(state: OrganizationState): Map<string, OrgUnit> {
    return new Map(state.units.map((unit) => [unit.id, unit]))
}

function membershipKey(state: OrganizationState): Set<string> {
    return new Set(
        state.memberships.map(
            (item) => `${item.user_id}:${item.org_unit_id}:${item.valid_from}`,
        ),
    )
}

function managementKey(state: OrganizationState): Set<string> {
    return new Set(state.management.map((item) => item.id))
}

export function impactChanges(
    receipt: OrganizationChangeReceipt,
    directory?: Pick<OrganizationStateView, "people" | "roles">,
): BusinessDiffEntry[] {
    const beforeUnits = unitMap(receipt.before)
    const changes: BusinessDiffEntry[] = []
    const people = directory?.people ?? []
    const roles = directory?.roles ?? []

    for (const unit of receipt.after.units) {
        const previous = beforeUnits.get(unit.id)
        if (!previous) {
            changes.push({
                id: `unit-add-${unit.id}`,
                field: unit.name,
                before: "无",
                after: `${KIND_LABEL[unit.kind]} · 启用`,
                note: "新建组织节点",
            })
            continue
        }
        if (previous.name !== unit.name) {
            changes.push({
                id: `unit-rename-${unit.id}`,
                field: "组织名称",
                before: previous.name,
                after: unit.name,
            })
        }
        if (previous.parent_id !== unit.parent_id) {
            changes.push({
                id: `unit-move-${unit.id}`,
                field: `${unit.name} 的上级`,
                before: previous.parent_id
                    ? unitLabel(receipt.before.units, previous.parent_id)
                    : "根节点",
                after: unit.parent_id
                    ? unitLabel(receipt.after.units, unit.parent_id)
                    : "根节点",
            })
        }
        if (previous.enabled && !unit.enabled) {
            changes.push({
                id: `unit-disable-${unit.id}`,
                field: unit.name,
                before: "启用",
                after: "停用",
                note: "历史节点保留，不删除业务事实",
            })
        }
    }

    const afterMemberships = membershipKey(receipt.after)
    for (const item of receipt.before.memberships) {
        const key = `${item.user_id}:${item.org_unit_id}:${item.valid_from}`
        if (afterMemberships.has(key)) continue
        changes.push({
            id: `member-end-${item.id}`,
            field: resolvePerson(people, item.user_id),
            before: unitLabel(receipt.before.units, item.org_unit_id),
            after: "结束主属关系",
            note: "不改派任务、不改写历史业绩",
        })
    }
    const beforeMemberships = membershipKey(receipt.before)
    for (const item of receipt.after.memberships) {
        const key = `${item.user_id}:${item.org_unit_id}:${item.valid_from}`
        if (beforeMemberships.has(key)) continue
        changes.push({
            id: `member-add-${item.id}`,
            field: resolvePerson(people, item.user_id),
            before: "无当前主属",
            after: unitLabel(receipt.after.units, item.org_unit_id),
            note: "调岗保留原关系记录",
        })
    }

    const afterGrants = managementKey(receipt.after)
    for (const item of receipt.before.management) {
        if (afterGrants.has(item.id)) continue
        changes.push({
            id: `grant-end-${item.id}`,
            field: `${resolvePerson(people, item.user_id)} · ${resolveRole(roles, item.role_id)}`,
            before: unitLabel(receipt.before.units, item.org_unit_id),
            after: "撤销管理范围",
            note: "不授予业务执行权",
        })
    }
    const beforeGrants = managementKey(receipt.before)
    for (const item of receipt.after.management) {
        if (beforeGrants.has(item.id)) continue
        changes.push({
            id: `grant-add-${item.id}`,
            field: `${resolvePerson(people, item.user_id)} · ${resolveRole(roles, item.role_id)}`,
            before: "无该管理范围",
            after: `${unitLabel(receipt.after.units, item.org_unit_id)}${
                item.include_descendants ? "（含下级）" : "（仅本级）"
            }`,
        })
    }

    if (changes.length === 0) {
        changes.push({
            id: "noop",
            field: OPERATION_LABEL[receipt.request.change.operation],
            before: "无变化",
            after: "无变化",
        })
    }
    return changes
}

export function impactCounts(changes: readonly BusinessDiffEntry[]) {
    return {
        estimated: changes.length,
        processable: changes.length,
        skipped: 0,
    }
}


