import type {
    DataScopeType,
    OrganizationOperation,
    OrgUnitKind,
    ScopeDimension,
    ScopeTargetMode,
} from "@/features/organization/types"

export const KIND_LABEL: Record<OrgUnitKind, string> = {
    department: "部门",
    team: "团队",
}

export const SCOPE_TYPE_LABEL: Record<DataScopeType, string> = {
    company: "公司级",
    organization: "组织",
    team: "团队",
    self_owned: "本人负责",
    collaborative: "协作参与",
}

export const DIMENSION_LABEL: Record<ScopeDimension, string> = {
    internal_org: "内部组织",
    settlement_party: "结算主体",
    warehouse: "仓库",
}

export const TARGET_MODE_LABEL: Record<ScopeTargetMode, string> = {
    explicit: "指定目标",
    own_org: "本人所属部门",
    managed_orgs: "本人管理的部门",
}

export const OPERATION_LABEL: Record<
    OrganizationOperation["operation"],
    string
> = {
    update_person_profile: "保存人员资料",
    create_unit: "新建组织",
    move_unit: "移动组织",
    rename_unit: "重命名组织",
    disable_unit: "停用组织",
    transfer_member: "调整所属部门",
    end_membership: "移出部门",
    grant_management: "设置管理部门",
    revoke_management: "撤销管理范围",
}

export const ORGANIZATION_BOUNDARY_NOTICE =
    "组织配置只调整内部组织和成员归属，不会改派任务，也不会授予销售、采购或审批等业务执行权。"

export const PAGE_NARROW_CLASS = "min-w-0 max-w-full overflow-x-hidden"

/** 非内部组织集合不能在界面被误读为部门范围。 */
export function scopeTypeLabel(
    scopeType: DataScopeType,
    dimension: ScopeDimension,
): string {
    if (scopeType === "organization" && dimension !== "internal_org") {
        return dimension === "settlement_party" ? "指定结算主体" : "指定仓库"
    }
    return SCOPE_TYPE_LABEL[scopeType]
}
