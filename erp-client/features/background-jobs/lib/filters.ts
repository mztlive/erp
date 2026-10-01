import { backgroundJobDomainLabel, JOB_TYPE_LABELS } from "../labels"

export const STATUS_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "active", label: "进行中" },
    { value: "pending", label: "等待执行" },
    { value: "running", label: "执行中" },
    { value: "partially_succeeded", label: "部分成功" },
    { value: "succeeded", label: "已完成" },
    { value: "failed", label: "执行失败" },
    { value: "cancelled", label: "已取消" },
] as const

export type StatusFilter = (typeof STATUS_FILTER_OPTIONS)[number]["value"]

export const SCOPE_FILTER_OPTIONS = [
    { value: "all", label: "全部任务" },
    { value: "mine", label: "只看我的" },
] as const

export type ScopeFilter = (typeof SCOPE_FILTER_OPTIONS)[number]["value"]

export type BackgroundJobFilterValues = {
    jobNo: string
    status: StatusFilter
    jobType: string
    domain: string
    scope: ScopeFilter
}

export const EMPTY_BACKGROUND_JOB_FILTERS: BackgroundJobFilterValues = {
    jobNo: "",
    status: "all",
    jobType: "",
    domain: "",
    scope: "all",
}

export const BACKGROUND_JOB_FILTER_CHIP_FIELDS: Record<
    string,
    keyof BackgroundJobFilterValues | undefined
> = {
    scope: "scope",
    job_no: "jobNo",
    status: "status",
    job_type: "jobType",
    domain: "domain",
}

/** 已应用条件的展示标签，不包含尚未提交的筛选草稿。 */
export function backgroundJobFilterChips(
    values: BackgroundJobFilterValues,
    isAdmin: boolean,
) {
    return [
        ...(isAdmin && values.scope === "mine"
            ? [
                  {
                      key: "scope",
                      label: "范围：只看我的",
                  },
              ]
            : []),
        ...(values.jobNo
            ? [
                  {
                      key: "job_no",
                      label: `任务号：${values.jobNo}`,
                  },
              ]
            : []),
        ...(values.status !== "all"
            ? [
                  {
                      key: "status",
                      label: `状态：${
                          STATUS_FILTER_OPTIONS.find(
                              (option) => option.value === values.status,
                          )?.label ?? values.status
                      }`,
                  },
              ]
            : []),
        ...(values.jobType
            ? [
                  {
                      key: "job_type",
                      label: `任务类型：${
                          JOB_TYPE_LABELS[values.jobType] ?? values.jobType
                      }`,
                  },
              ]
            : []),
        ...(values.domain
            ? [
                  {
                      key: "domain",
                      label: `业务类型：${backgroundJobDomainLabel(values.domain, null)}`,
                  },
              ]
            : []),
    ]
}
