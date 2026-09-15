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
    own_org: "本人组织",
    managed_orgs: "管理的组织",
}

export const OPERATION_LABEL: Record<
    OrganizationOperation["operation"],
    string
> = {
    create_unit: "新建组织",
    move_unit: "移动组织",
    rename_unit: "重命名组织",
    disable_unit: "停用组织",
    transfer_member: "成员调岗",
    end_membership: "结束成员关系",
    grant_management: "授予管理范围",
    revoke_management: "撤销管理范围",
}

export const ORGANIZATION_BOUNDARY_NOTICE =
    "组织配置只调整内部组织、成员与管理关系，不会改派任务，也不会授予销售、采购或审批等业务执行权。"

export const MANAGEMENT_GRANT_NOTICE =
    "管理授权必须显式指定角色和组织，不能因为对方是部门负责人就自动获得组织配置权。"

export const PAGE_NARROW_CLASS = "min-w-0 max-w-full overflow-x-hidden"
